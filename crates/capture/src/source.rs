//! What a capture source hands out, independent of the platform (spec 0019).

use std::time::Duration;

use mixlink_core::clock::Ticks;

use crate::CaptureError;

/// One captured frame. Dropping it gives the frame back to the driver (the surface may be reused).
pub trait Frame {
    fn seq(&self) -> u64;
    fn width(&self) -> u32;
    fn height(&self) -> u32;
    /// DWM present time (same clock as `mixlink_core::clock`).
    fn present_qpc(&self) -> Ticks;
    /// Driver acquire time.
    fn acquire_qpc(&self) -> Ticks;
    /// When the reader had the frame ready for the encoder (`Stage::Captured`).
    fn captured(&self) -> Ticks;
}

/// Result of one wait on a source.
#[derive(Debug)]
pub enum CaptureEvent<F> {
    Frame(F),
    /// The frame size or swapchain changed: re-create whatever depends on it (encoder), start with an IDR.
    Reopened {
        width: u32,
        height: u32,
    },
}

pub trait FrameSource {
    type Frame<'a>: Frame
    where
        Self: 'a;

    // HOT PATH
    /// Waits up to `timeout` for the next frame. `Ok(None)`: nothing new (static screen).
    fn next(
        &mut self,
        timeout: Duration,
    ) -> Result<Option<CaptureEvent<Self::Frame<'_>>>, CaptureError>;
}
