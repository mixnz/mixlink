//! Safe presenter API over the platform swapchain.

use std::fmt;
use std::sync::Arc;

use raw_window_handle::{HasWindowHandle, RawWindowHandle};

use crate::RenderError;
use crate::platform::windows::d3d11::D3d11Swapchain;

/// Clears and presents into one window. Redraw-driven in T1; the per-frame loop comes with T8.
pub struct Presenter {
    swapchain: D3d11Swapchain,
    // Keeps the window (and its HWND) alive for as long as the swapchain uses it.
    _window: Arc<dyn HasWindowHandle + Send + Sync>,
}

impl Presenter {
    pub fn new<W: HasWindowHandle + Send + Sync + 'static>(
        window: Arc<W>,
        width: u32,
        height: u32,
    ) -> Result<Self, RenderError> {
        let hwnd = match window.window_handle()?.as_raw() {
            RawWindowHandle::Win32(handle) => handle.hwnd,
            _ => return Err(RenderError::UnsupportedWindow),
        };
        let swapchain = D3d11Swapchain::new(hwnd, width, height)?;
        Ok(Self {
            swapchain,
            _window: window,
        })
    }

    /// A zero width or height (minimized window) pauses presenting until the next non-zero size.
    pub fn resize(&mut self, width: u32, height: u32) -> Result<(), RenderError> {
        self.swapchain.resize(width, height)
    }

    pub fn clear_and_present(&mut self, rgba: [f32; 4]) -> Result<(), RenderError> {
        self.swapchain.clear_and_present(rgba)
    }
}

impl fmt::Debug for Presenter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Presenter").finish_non_exhaustive()
    }
}
