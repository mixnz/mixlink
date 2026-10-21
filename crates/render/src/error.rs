//! Render errors.

use raw_window_handle::HandleError;

#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("window handle is not a Win32 HWND")]
    UnsupportedWindow,
    #[error("window handle unavailable: {0}")]
    Handle(#[from] HandleError),
    #[cfg(windows)]
    #[error("{context} failed: {source}")]
    Win32 {
        context: &'static str,
        #[source]
        source: windows::core::Error,
    },
}
