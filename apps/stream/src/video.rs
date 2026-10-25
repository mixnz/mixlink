//! The `video` thread: owns the presenter and the decoder, decodes every queued access unit and
//! presents only the newest frame (spec 0013). The winit thread only passes sizes in.

use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use mixlink_codec::{CodecError, DecodedFrame, Decoder, VideoCodec};
use mixlink_core::ids::FrameId;
use mixlink_core::mmcss::{Mmcss, MmcssTask};
use mixlink_core::stage::{Stage, StageStamps};
use mixlink_render::{Presenter, VideoFrame};
use winit::raw_window_handle::HasWindowHandle;

use crate::au_queue::AuReceiver;
use crate::stats::{FrameStats, STATS_FRAMES};

/// Shown until the first video frame (and without video).
const IDLE_COLOR: [f32; 4] = [0.05, 0.25, 0.30, 1.0];
/// Longest sleep with nothing to do; resize and stop also wake the thread.
const IDLE_WAIT: Duration = Duration::from_millis(16);

/// What the video thread reports to the window thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VideoEvent {
    /// The window shows something (first present done).
    FirstPresent,
    /// Presenter or decoder failed; the thread has ended.
    Failed(String),
}

/// Access units to decode (play mode now, network later).
#[derive(Debug)]
pub struct VideoSource {
    pub rx: AuReceiver,
    pub codec: VideoCodec,
    /// Wait for the GPU after each decode so `DecodeEnd` is GPU-complete.
    pub measure: bool,
}

#[derive(Debug, Default)]
struct Shared {
    stop: AtomicBool,
    /// Pending window size `(w << 32) | h`, valid while `redraw` is set.
    size: AtomicU64,
    redraw: AtomicBool,
}

#[derive(Debug)]
pub struct VideoThread {
    shared: Arc<Shared>,
    handle: JoinHandle<FrameStats>,
}

impl VideoThread {
    pub fn start<W: HasWindowHandle + Send + Sync + 'static>(
        window: Arc<W>,
        size: (u32, u32),
        source: Option<VideoSource>,
        mut events: impl FnMut(VideoEvent) + Send + 'static,
    ) -> io::Result<VideoThread> {
        let shared = Arc::new(Shared::default());
        let inner = Arc::clone(&shared);
        let handle = thread::Builder::new().name("video".into()).spawn(move || {
            let _mmcss = Mmcss::enter(MmcssTask::Games);
            let mut stats = FrameStats::new(STATS_FRAMES);
            if let Err(message) = run(window, size, source, &inner, &mut events, &mut stats) {
                log::error!("video thread: {message}");
                events(VideoEvent::Failed(message));
            }
            stats
        })?;
        Ok(VideoThread { shared, handle })
    }

    /// New window size (0 x 0 = minimized); the thread resizes and redraws.
    pub fn resize(&self, width: u32, height: u32) {
        let packed = (u64::from(width) << 32) | u64::from(height);
        self.shared.size.store(packed, Ordering::Relaxed);
        self.shared.redraw.store(true, Ordering::Release);
        self.handle.thread().unpark();
    }

    /// Ends the thread and returns its stats (`None` if it panicked).
    pub fn stop(self) -> Option<FrameStats> {
        self.shared.stop.store(true, Ordering::Relaxed);
        self.handle.thread().unpark();
        self.handle.join().ok()
    }
}

fn run<W: HasWindowHandle + Send + Sync + 'static>(
    window: Arc<W>,
    size: (u32, u32),
    source: Option<VideoSource>,
    shared: &Shared,
    events: &mut impl FnMut(VideoEvent),
    stats: &mut FrameStats,
) -> Result<(), String> {
    let mut presenter = Presenter::new(window, size.0, size.1).map_err(|e| e.to_string())?;
    let mut video = match source {
        Some(source) => {
            source.rx.attach_current_thread();
            let decoder =
                Decoder::new(&presenter.device(), source.codec).map_err(|e| e.to_string())?;
            Some((source, decoder))
        }
        None => None,
    };
    presenter
        .clear_and_present(IDLE_COLOR)
        .map_err(|e| e.to_string())?;
    events(VideoEvent::FirstPresent);
    let mut shown: Option<DecodedFrame> = None;
    let mut frame_id = 0u32;
    while !shared.stop.load(Ordering::Relaxed) {
        let mut redraw = false;
        if shared.redraw.swap(false, Ordering::Acquire) {
            let packed = shared.size.load(Ordering::Relaxed);
            let (w, h) = ((packed >> 32) as u32, packed as u32);
            presenter.resize(w, h).map_err(|e| e.to_string())?;
            redraw = true;
        }
        let newest = match video.as_mut() {
            Some((source, decoder)) => {
                decode_queued(source, decoder, &mut presenter, &mut frame_id, stats)?
            }
            None => None,
        };
        if let Some((frame, mut stamps)) = newest {
            present(&mut presenter, &frame)?;
            stamps.mark(Stage::Presented);
            stats.record(&stamps);
            stats.presented += 1;
            shown = Some(frame);
        } else if redraw {
            match &shown {
                Some(frame) => present(&mut presenter, frame)?,
                None => presenter
                    .clear_and_present(IDLE_COLOR)
                    .map_err(|e| e.to_string())?,
            }
        } else {
            thread::park_timeout(IDLE_WAIT);
        }
    }
    Ok(())
}

/// Decodes every queued AU (P-frames need all of them); keeps only the newest frame.
// HOT PATH
fn decode_queued(
    source: &mut VideoSource,
    decoder: &mut Decoder,
    presenter: &mut Presenter,
    frame_id: &mut u32,
    stats: &mut FrameStats,
) -> Result<Option<(DecodedFrame, StageStamps)>, String> {
    let mut newest = None;
    while let Some(au) = source.rx.try_recv() {
        let mut stamps = StageStamps::new(FrameId::new(*frame_id));
        *frame_id = frame_id.wrapping_add(1);
        stamps.mark_at(Stage::Received, au.received());
        stamps.mark(Stage::DecodeStart);
        match decoder.decode(au.data()) {
            Ok(Some(frame)) => {
                if source.measure {
                    presenter.wait_gpu();
                }
                stamps.mark(Stage::DecodeEnd);
                stats.decoded += 1;
                if newest.replace((frame, stamps)).is_some() {
                    stats.dropped += 1;
                }
            }
            Ok(None) => {}
            Err(CodecError::Corrupt) => stats.corrupt += 1,
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(newest)
}

// HOT PATH
fn present(presenter: &mut Presenter, frame: &DecodedFrame) -> Result<(), String> {
    let view = VideoFrame {
        texture: frame.texture(),
        array_index: frame.array_index(),
        width: frame.width(),
        height: frame.height(),
    };
    presenter.present_nv12(&view).map_err(|e| e.to_string())
}

#[cfg(test)]
#[path = "video_tests.rs"]
mod tests;
