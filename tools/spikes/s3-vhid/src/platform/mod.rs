//! Win32 side of the harness. Every `unsafe` block carries a `// SAFETY:` note.

pub mod devices;
pub mod hid;
pub mod poll;
pub mod qpc;
pub mod rawinput;
pub mod sendinput;
pub mod timer;
pub mod vigem;

use crate::feed::{Backend, FeedError, Feeder};

/// Opens the feeder for `backend`.
pub fn open_feeder(backend: Backend) -> Result<Box<dyn Feeder>, FeedError> {
    Ok(match backend {
        Backend::Vhid => Box::new(hid::VhidFeeder::open()?),
        Backend::SendInput => Box::new(sendinput::SendInputFeeder),
        Backend::Vigem => Box::new(vigem::VigemFeeder::connect()?),
    })
}

pub(crate) fn os_err(call: &str, e: windows::core::Error) -> FeedError {
    FeedError::Os(format!("{call}: {e}"))
}

/// Raises the calling thread's priority for timing runs (best effort).
pub fn set_time_critical() {
    use windows::Win32::System::Threading::{
        GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_TIME_CRITICAL,
    };
    // SAFETY: pseudo-handle of the current thread; no memory is passed.
    let _ = unsafe { SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_TIME_CRITICAL) };
}
