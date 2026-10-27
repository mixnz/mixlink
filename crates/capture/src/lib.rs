//! Host screen capture into D3D11 textures: the `vdisplay` frame share (IddCx surface + fence, spec 0019).

pub mod error;
pub mod header;
pub mod reader;
pub mod surface_cache;

pub use error::CaptureError;
