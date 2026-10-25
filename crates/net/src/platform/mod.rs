//! Platform socket fixes, picked at compile time.

#[cfg(not(windows))]
mod fallback;
#[cfg(windows)]
mod windows;

#[cfg(not(windows))]
pub(crate) use fallback::disable_udp_connreset;
#[cfg(windows)]
pub(crate) use windows::socket::disable_udp_connreset;
