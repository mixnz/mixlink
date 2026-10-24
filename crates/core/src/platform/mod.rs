//! Platform clock source, picked at compile time.

#[cfg(not(windows))]
mod fallback;
#[cfg(windows)]
mod windows;

#[cfg(not(windows))]
pub(crate) use fallback::{now_raw, ticks_per_sec};
#[cfg(windows)]
pub(crate) use windows::qpc::{now_raw, ticks_per_sec};
