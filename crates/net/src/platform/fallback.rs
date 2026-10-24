//! Non-Windows stand-ins: nothing to fix, no MMCSS.

use std::io;
use std::net::UdpSocket;

pub(crate) fn disable_udp_connreset(_socket: &UdpSocket) -> io::Result<()> {
    Ok(())
}

#[derive(Debug)]
pub(crate) struct Mmcss;

impl Mmcss {
    pub(crate) fn enter() -> Option<Mmcss> {
        None
    }
}
