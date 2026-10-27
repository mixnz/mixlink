//! Render errors.

use raw_window_handle::HandleError;

#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("window handle is not a Win32 HWND")]
    UnsupportedWindow,
    #[error("window handle unavailable: {0}")]
    Handle(#[from] HandleError),
    #[error("cursor shape {width}x{height}, pitch {pitch}, {len} bytes is not usable")]
    CursorShape {
        width: u32,
        height: u32,
        pitch: u32,
        len: usize,
    },
    #[cfg(windows)]
    #[error("{context} failed: {source}")]
    Win32 {
        context: &'static str,
        #[source]
        source: windows::core::Error,
    },
}
