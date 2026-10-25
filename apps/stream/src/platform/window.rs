//! Window handle for the video thread (spec 0013). winit only hands out the HWND on the event
//! loop thread; the swapchain lives on the video thread.

use std::sync::Arc;

use winit::platform::windows::{AnyThread, WindowBorrowExtWindows};
use winit::window::Window;

/// `window` usable as a raw handle from any thread.
pub(crate) fn any_thread(window: &Arc<Window>) -> AnyThread<Arc<Window>> {
    // SAFETY: the handle only feeds the DXGI swapchain (flip model, created, resized and presented
    // on the video thread, which Windows allows); every window message is still handled by the
    // event loop thread, and the `Arc` keeps the window alive until the video thread has stopped.
    unsafe { Arc::clone(window).any_thread() }
}
