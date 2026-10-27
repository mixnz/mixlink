//! Host screen capture into D3D11 textures: the `vdisplay` frame share (IddCx surface + fence, spec 0019).

pub mod error;
pub mod header;
mod platform;
pub mod reader;
pub mod source;
pub mod surface_cache;

pub use error::CaptureError;
#[cfg(windows)]
pub use platform::windows::idd_source::{CapturedFrame, IddSource};
pub use source::{CaptureEvent, Frame, FrameSource};
