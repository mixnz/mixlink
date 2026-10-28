//! The network `EncodeSink` (spec 0020): NVENC on the capture device, each access unit with its
//! `FrameMeta` to the client; feedback is drained once per frame.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use mixlink_capture::{CaptureError, CapturedFrame, Frame, IddSource};
use mixlink_codec::{Encoder, EncoderConfig};
use mixlink_core::clock::{self, Ticks};
use mixlink_net::{RxLane, TxLane};
use mixlink_proto::video::FrameMeta;

use super::feedback::FeedbackRx;
use super::video_send::VideoSender;
use crate::capture::flow::{Delivery, EncodeSink};
use crate::error::SvcError;

#[derive(Debug)]
pub struct NetSink {
    config: EncoderConfig,
    encoder: Option<Encoder>,
    video: VideoSender,
    feedback: FeedbackRx,
    /// Set when the client asked for a cursor shape; `svc-cursor` takes it.
    shape_request: Arc<AtomicBool>,
    /// Frame gaps (driver acquire to acquire): DWM sends frames only on change; long ones show a stall.
    last_acquire: Option<Ticks>,
    longest_gap: Duration,
    long_gaps: u32,
}

/// A gap this long between frames counts as a stall in the session log.
const LONG_GAP: Duration = Duration::from_millis(250);

impl NetSink {
    /// `config.width` / `height` are replaced by each reopen's size.
    pub fn new(
        config: EncoderConfig,
        video: VideoSender,
        feedback: FeedbackRx,
        shape_request: Arc<AtomicBool>,
    ) -> Self {
        Self {
            config,
            encoder: None,
            video,
            feedback,
            shape_request,
            last_acquire: None,
            longest_gap: Duration::ZERO,
            long_gaps: 0,
        }
    }

    pub fn longest_gap(&self) -> Duration {
        self.longest_gap
    }

    pub fn long_gaps(&self) -> u32 {
        self.long_gaps
    }

    pub fn into_lanes(self) -> (TxLane, RxLane) {
        (self.video.into_lane(), self.feedback.into_lane())
    }
}

fn micros(d: Duration) -> u32 {
    u32::try_from(d.as_micros()).unwrap_or(u32::MAX)
}

impl EncodeSink<IddSource> for NetSink {
    fn reopen(&mut self, src: &IddSource, width: u32, height: u32) -> Result<(), SvcError> {
        self.encoder = None;
        let device = src.device().ok_or(CaptureError::NoSwapchain)?;
        self.config.width = width;
        self.config.height = height;
        self.encoder = Some(Encoder::new(&device, &self.config)?);
        Ok(())
    }

    // HOT PATH
    fn encode(&mut self, frame: &CapturedFrame<'_>, force_idr: bool) -> Result<Delivery, SvcError> {
        let acquired = frame.acquire_qpc();
        if let Some(last) = self.last_acquire.replace(acquired) {
            let gap = acquired.duration_since(last);
            self.longest_gap = self.longest_gap.max(gap);
            self.long_gaps += u32::from(gap >= LONG_GAP);
        }
        let asked = self.feedback.poll(Instant::now());
        if asked.shape_request {
            self.shape_request.store(true, Ordering::Release);
        }
        let Some(encoder) = self.encoder.as_mut() else {
            return Ok(Delivery::Dropped(0));
        };
        let idr = force_idr || asked.force_idr;
        let start = clock::now();
        let au = encoder.encode(frame.texture(), idr)?;
        let meta = FrameMeta {
            keyframe: idr,
            capture_us: micros(frame.captured().duration_since(frame.acquire_qpc())),
            encode_us: micros(clock::now().duration_since(start)),
            send_us: 0,
        };
        Ok(self.video.send_au(&meta, au))
    }
}
