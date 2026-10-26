//! Non-Windows stand-in: nothing to fix, no interface list.

use std::io;
use std::net::UdpSocket;

use crate::discovery::error::DiscoveryError;
use crate::discovery::local_net::LocalNet;

pub(crate) fn disable_udp_connreset(_socket: &UdpSocket) -> io::Result<()> {
    Ok(())
}

pub(crate) fn local_ipv4_nets() -> Result<Vec<LocalNet>, DiscoveryError> {
    Err(DiscoveryError::Unsupported)
}
