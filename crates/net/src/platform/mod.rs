//! Platform socket fixes and interface queries, picked at compile time.

#[cfg(not(windows))]
mod fallback;
#[cfg(windows)]
mod windows;

#[cfg(not(windows))]
pub(crate) use fallback::{disable_udp_connreset, local_ipv4_nets};
#[cfg(windows)]
pub(crate) use windows::adapters::local_ipv4_nets;
#[cfg(windows)]
pub(crate) use windows::socket::disable_udp_connreset;
