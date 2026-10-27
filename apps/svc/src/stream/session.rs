//! The body of `svc-video` (spec 0020): monitor on, `svc-cursor` + capture → NVENC → network until the
//! session stops, monitor off. Windows only; the orchestration around it is tested in `streamer`.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use mixlink_capture::{CursorSource, IddSource};
use mixlink_codec::{EncoderConfig, VideoCodec};
use mixlink_proto::control::CodecId;

use super::cursor_send::CursorSender;
use super::feedback::FeedbackRx;
use super::net_sink::NetSink;
use super::streamer::{StreamEnd, StreamLanes, StreamStart, run_session};
use super::video_send::VideoSender;
use crate::capture::flow::run_loop;
use crate::capture::report::Stats;
use crate::capture_args::Levers;
use crate::display::requests::DisplayHandle;
use crate::error::SvcError;
use crate::platform::windows::priority::apply_thread;

/// Frames whose timings are kept for the session log (~10 min at 120 Hz); later ones are only counted.
const STATS_FRAMES: usize = 72_000;
/// One wait for a cursor update; also how often stop is checked.
const CURSOR_WAIT: Duration = Duration::from_millis(50);
/// An idle cursor's position is sent again this often, so one lost packet does not leave it wrong.
const POSITION_REFRESH: Duration = Duration::from_millis(250);
/// Shortest time between two shape resends asked by the client.
const SHAPE_RESEND_GAP: Duration = Duration::from_millis(100);
/// No deadline: the session's stop flag ends the loop.
const SESSION_MAX: Duration = Duration::from_secs(365 * 24 * 3600);

/// Runs one session's stream; the lanes always come back.
pub fn run_stream(
    display: &DisplayHandle,
    start: &StreamStart,
    lanes: StreamLanes,
    stop: &AtomicBool,
) -> (StreamLanes, StreamEnd) {
    log::info!(
        "streaming to {} at {}: {}x{} at {} mHz, {:?}",
        start.device,
        start.peer,
        start.mode.width,
        start.mode.height,
        start.mode.refresh_mhz,
        start.codec
    );
    run_session(display, start, lanes, |connector, lanes| {
        stream_on(connector, start, lanes, stop)
    })
}

fn stream_on(
    connector: u32,
    start: &StreamStart,
    lanes: StreamLanes,
    stop: &AtomicBool,
) -> (StreamLanes, Result<(), SvcError>) {
    let StreamLanes {
        video,
        cursor,
        feedback,
    } = lanes;
    let shape_request = Arc::new(AtomicBool::new(false));
    let cursor_stop = AtomicBool::new(false);
    let mut cursor = CursorSender::new(cursor, &start.keys, start.peer);
    let mut sink = NetSink::new(
        encoder_config(start),
        VideoSender::new(video, &start.keys, start.peer),
        FeedbackRx::new(feedback, &start.keys, start.peer),
        Arc::clone(&shape_request),
    );
    let captured = thread::scope(|scope| {
        let spawned = thread::Builder::new()
            .name("svc-cursor".into())
            .spawn_scoped(scope, || {
                cursor_loop(connector, &mut cursor, &cursor_stop, &shape_request);
            });
        if let Err(error) = spawned {
            log::warn!("cursor thread not started, no cursor on the client: {error}");
        }
        let captured = capture(connector, &mut sink, stop);
        cursor_stop.store(true, Ordering::Release);
        captured
    });
    let (video, feedback) = sink.into_lanes();
    let lanes = StreamLanes {
        video,
        cursor: cursor.into_lane(),
        feedback,
    };
    (lanes, captured)
}

fn encoder_config(start: &StreamStart) -> EncoderConfig {
    EncoderConfig {
        codec: match start.codec {
            CodecId::H264 => VideoCodec::H264,
            CodecId::Hevc => VideoCodec::Hevc,
        },
        width: start.mode.width,
        height: start.mode.height,
        fps: start.mode.refresh_mhz.div_ceil(1000).max(1),
        bitrate_kbps: start.video.bitrate_kbps,
    }
}

/// Capture → encode → send until stop (spec 0019 levers: MMCSS + priority class, no GPU priority).
fn capture(connector: u32, sink: &mut NetSink, stop: &AtomicBool) -> Result<(), SvcError> {
    let _levers = apply_thread(Levers {
        mmcss: true,
        gpu: false,
        class: true,
    });
    let mut src = IddSource::open(connector)?;
    let mut stats = Stats::with_capacity(STATS_FRAMES);
    let deadline = Instant::now() + SESSION_MAX;
    let ran = run_loop(
        &mut src,
        sink,
        &|| stop.load(Ordering::Acquire),
        deadline,
        &mut stats,
    );
    let us = |ns: u64| ns / 1000;
    log::info!(
        "stream ended: {} frames, {} IDRs, {} dropped, {} reopens; capture p50/p99 {}/{} us, encode \
         p50/p99 {}/{} us; hold timeouts {}, ring fallbacks {}",
        stats.frames,
        stats.idrs,
        stats.dropped_aus,
        stats.reopens,
        us(stats.capture.percentile(50.0)),
        us(stats.capture.percentile(99.0)),
        us(stats.encode.percentile(50.0)),
        us(stats.encode.percentile(99.0)),
        src.hold_timeouts(),
        src.ring_fallbacks()
    );
    ran
}

/// `svc-cursor`: every cursor update to the client; a missing share leaves the cursor in the video.
fn cursor_loop(
    connector: u32,
    sender: &mut CursorSender,
    stop: &AtomicBool,
    shape_request: &AtomicBool,
) {
    let mut source = match CursorSource::open(connector) {
        Ok(source) => source,
        Err(error) => {
            log::warn!("no cursor share, the cursor stays in the video: {error}");
            return;
        }
    };
    let mut last_resend: Option<Instant> = None;
    let mut last_pos = None;
    let mut last_sent = Instant::now();
    while !stop.load(Ordering::Acquire) {
        let now = Instant::now();
        let may_resend = last_resend.is_none_or(|at| now.duration_since(at) >= SHAPE_RESEND_GAP);
        if may_resend && shape_request.swap(false, Ordering::AcqRel) {
            source.resend_shape();
            last_resend = Some(now);
        }
        match source.next(CURSOR_WAIT) {
            Some(update) => {
                if let Some((info, pixels)) = update.shape {
                    sender.send_shape(&info, update.pos.shape_id, pixels);
                }
                sender.send_position(&update.pos);
                last_pos = Some(update.pos);
                last_sent = Instant::now();
            }
            None => {
                if let Some(pos) = last_pos
                    && last_sent.elapsed() >= POSITION_REFRESH
                {
                    sender.send_position(&pos);
                    last_sent = Instant::now();
                }
            }
        }
    }
}
