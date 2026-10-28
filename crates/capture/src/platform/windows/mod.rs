//! Reader of the `vdisplay` frame share (spec 0019).

pub(crate) mod cursor_source;
pub(crate) mod device;
pub(crate) mod idd_source;
pub(crate) mod section;

use crate::CaptureError;

pub(crate) fn win32(context: &'static str) -> impl FnOnce(windows::core::Error) -> CaptureError {
    move |source| CaptureError::Win32 { context, source }
}
