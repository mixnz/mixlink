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

#[derive(Debug)]
pub(crate) struct MdnsRegistration;

impl MdnsRegistration {
    pub(crate) fn register(
        _advert: &crate::discovery::advert::Advert,
    ) -> Result<MdnsRegistration, DiscoveryError> {
        Err(DiscoveryError::Unsupported)
    }
}

#[derive(Debug)]
pub(crate) struct MdnsBrowse;

impl MdnsBrowse {
    pub(crate) fn start(
        _tx: std::sync::mpsc::Sender<crate::discovery::browser::DiscoveryEvent>,
    ) -> Result<MdnsBrowse, DiscoveryError> {
        Err(DiscoveryError::Unsupported)
    }

    pub(crate) fn poll(&mut self, _refresh: bool) {}
}
