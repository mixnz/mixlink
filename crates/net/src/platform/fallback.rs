//! Non-Windows stand-ins: nothing to fix.

use std::io;
use std::net::UdpSocket;

pub(crate) fn disable_udp_connreset(_socket: &UdpSocket) -> io::Result<()> {
    Ok(())
}
