//! The capture → encode loop of `capture try` (spec 0019), generic so tests drive it with fakes.

use std::time::{Duration, Instant};

use mixlink_capture::{CaptureEvent, Frame, FrameSource};
use mixlink_core::clock;

use super::report::Stats;
use crate::error::SvcError;

/// How long one wait for a frame lasts; also how often stop and the deadline are checked.
pub const FRAME_WAIT: Duration = Duration::from_millis(100);

/// What became of one encoded access unit (its size in bytes either way).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delivery {
    Sent(usize),
    /// Not forwarded (writer full): the next frame must be an IDR, every P-frame needs the one before.
    Dropped(usize),
}

/// Where frames go: the encoder (+ file today, the network later).
pub trait EncodeSink<S: FrameSource> {
    /// A new frame size or swapchain: re-create the encoder for `width` x `height` on `src`'s device.
    fn reopen(&mut self, src: &S, width: u32, height: u32) -> Result<(), SvcError>;
    /// Encodes and forwards one frame.
    fn encode(&mut self, frame: &S::Frame<'_>, force_idr: bool) -> Result<Delivery, SvcError>;
}

/// Runs until `should_stop` or `deadline`. IDRs: the first frame, the first after each reopen, and the
/// one after any access unit that was not forwarded.
pub fn run_loop<S: FrameSource, E: EncodeSink<S>>(
    src: &mut S,
    sink: &mut E,
    should_stop: &dyn Fn() -> bool,
    deadline: Instant,
    stats: &mut Stats,
) -> Result<(), SvcError> {
    let mut force_idr = true;
    while !should_stop() && Instant::now() < deadline {
        // The event borrows `src`; it is dropped (frame released) before `reopen` needs `src`.
        let reopen = match src.next(FRAME_WAIT)? {
            None => None,
            Some(CaptureEvent::Reopened { width, height }) => Some((width, height)),
            Some(CaptureEvent::Frame(frame)) => {
                force_idr = !encode_one(sink, &frame, force_idr, stats)?;
                None
            }
        };
        if let Some((width, height)) = reopen {
            sink.reopen(src, width, height)?;
            stats.reopens += 1;
            force_idr = true;
        }
    }
    Ok(())
}

// HOT PATH
/// `true` when the access unit was forwarded.
fn encode_one<S: FrameSource, E: EncodeSink<S>>(
    sink: &mut E,
    frame: &S::Frame<'_>,
    force_idr: bool,
    stats: &mut Stats,
) -> Result<bool, SvcError> {
    let start = clock::now();
    let delivery = sink.encode(frame, force_idr)?;
    let end = clock::now();
    stats.frames += 1;
    if force_idr {
        stats.idrs += 1;
    }
    let (bytes, sent) = match delivery {
        Delivery::Sent(bytes) => (bytes, true),
        Delivery::Dropped(bytes) => {
            stats.dropped_aus += 1;
            (bytes, false)
        }
    };
    let nanos = |d: Duration| u64::try_from(d.as_nanos()).unwrap_or(u64::MAX);
    stats
        .capture
        .push(nanos(frame.captured().duration_since(frame.present_qpc())));
    stats.encode.push(nanos(end.duration_since(start)));
    stats.au_bytes.push(bytes as u64);
    Ok(sent)
}

#[cfg(test)]
#[path = "flow_tests.rs"]
mod tests;
