//! Platform clock source and thread priority, picked at compile time.

#[cfg(not(windows))]
mod fallback;
#[cfg(windows)]
mod windows;

#[cfg(not(windows))]
pub use fallback::Mmcss;
#[cfg(not(windows))]
pub(crate) use fallback::{now_raw, ticks_per_sec};
#[cfg(windows)]
pub use windows::mmcss::Mmcss;
#[cfg(windows)]
pub(crate) use windows::qpc::{now_raw, ticks_per_sec};
