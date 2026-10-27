//! Host side of discovery (spec 0017): registers the mDNS service and answers broadcast probes
//! on the discovery port.

use std::io::ErrorKind;
use std::net::{Ipv4Addr, UdpSocket};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::discovery::advert::Advert;
use crate::discovery::error::DiscoveryError;
use crate::discovery::limiter::ReplyLimiter;
use crate::discovery::wire::{encode_reply, is_probe};
use crate::platform::{MdnsRegistration, disable_udp_connreset};

/// How often the responder checks for a stop.
const STOP_POLL: Duration = Duration::from_millis(200);
/// Room for any datagram; anything larger than a probe is ignored anyway.
const RECV_BUF: usize = 1500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdvertiserConfig {
    /// 0 picks a free port (tests).
    pub discovery_port: u16,
    pub mdns: bool,
}

#[derive(Debug)]
pub struct Advertiser {
    stop: Arc<AtomicBool>,
    responder: Option<JoinHandle<()>>,
    port: Option<u16>,
    /// Dropping it deregisters (mDNS goodbye).
    _mdns: Option<MdnsRegistration>,
}

impl Advertiser {
    /// Starts what it can; fails only when nothing started.
    pub fn start(advert: Advert, cfg: AdvertiserConfig) -> Result<Advertiser, DiscoveryError> {
        let mut reply = Vec::new();
        encode_reply(&advert, &mut reply)?;
        let stop = Arc::new(AtomicBool::new(false));
        let (responder, port) = match start_responder(reply, cfg.discovery_port, &stop) {
            Ok((handle, port)) => (Some(handle), Some(port)),
            Err(error) => {
                log::error!(
                    "discovery responder on port {} not started: {error}",
                    cfg.discovery_port
                );
                (None, None)
            }
        };
        let mdns = cfg
            .mdns
            .then(|| MdnsRegistration::register(&advert))
            .and_then(|result| {
                result
                    .inspect(|_| log::info!("mDNS service registered"))
                    .inspect_err(|error| log::error!("mDNS registration failed: {error}"))
                    .ok()
            });
        if responder.is_none() && mdns.is_none() {
            return Err(DiscoveryError::NothingStarted);
        }
        Ok(Advertiser {
            stop,
            responder,
            port,
            _mdns: mdns,
        })
    }

    /// The responder's port, `None` when it did not start.
    pub fn local_port(&self) -> Option<u16> {
        self.port
    }

    pub fn stop(self) {}
}

impl Drop for Advertiser {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.responder.take()
            && handle.join().is_err()
        {
            log::error!("discovery responder panicked");
        }
    }
}

fn start_responder(
    reply: Vec<u8>,
    port: u16,
    stop: &Arc<AtomicBool>,
) -> Result<(JoinHandle<()>, u16), DiscoveryError> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, port))?;
    socket.set_read_timeout(Some(STOP_POLL))?;
    if let Err(error) = disable_udp_connreset(&socket) {
        log::debug!("SIO_UDP_CONNRESET not disabled: {error}");
    }
    let port = socket.local_addr()?.port();
    let stop = Arc::clone(stop);
    let handle = thread::Builder::new()
        .name("svc-discovery".into())
        .spawn(move || respond(&socket, &reply, &stop))?;
    log::info!("discovery responder on UDP port {port}");
    Ok((handle, port))
}

fn respond(socket: &UdpSocket, reply: &[u8], stop: &AtomicBool) {
    let mut limiter = ReplyLimiter::new();
    let mut buf = [0u8; RECV_BUF];
    while !stop.load(Ordering::Relaxed) {
        match socket.recv_from(&mut buf) {
            Ok((len, src)) => {
                let Some(probe) = buf.get(..len) else {
                    continue;
                };
                if is_probe(probe)
                    && limiter.allow(src.ip(), Instant::now())
                    && let Err(error) = socket.send_to(reply, src)
                {
                    log::debug!("discovery reply to {src} failed: {error}");
                }
            }
            // A closed peer (ICMP port unreachable) or the stop poll: keep serving.
            Err(error)
                if matches!(
                    error.kind(),
                    ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::ConnectionReset
                ) => {}
            Err(error) => log::debug!("discovery responder receive failed: {error}"),
        }
    }
}

#[cfg(test)]
#[path = "advertiser_tests.rs"]
mod tests;
