//! Platform socket fixes and thread priority, picked at compile time.

#[cfg(not(windows))]
mod fallback;
#[cfg(windows)]
mod windows;

#[cfg(not(windows))]
pub(crate) use fallback::{Mmcss, disable_udp_connreset};
#[cfg(windows)]
pub(crate) use windows::{mmcss::Mmcss, socket::disable_udp_connreset};
