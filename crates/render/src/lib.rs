//! Client presentation: D3D11 flip-model swapchain, waitable object, tearing/VRR, newest-frame-wins.

mod error;
#[cfg(windows)]
mod platform;
#[cfg(windows)]
mod presenter;

pub use error::RenderError;
#[cfg(windows)]
pub use presenter::Presenter;
