//! Client side of discovery (spec 0017): probes the LAN, merges replies (and mDNS events) into a
//! `HostList`, and reports each change.

use std::io::ErrorKind;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use mixlink_core::ids::DeviceId;

use crate::discovery::advert::Advert;
use crate::discovery::error::DiscoveryError;
use crate::discovery::list::{HostEntry, HostList, Source};
use crate::discovery::local_net::LocalNet;
use crate::discovery::wire::{MAX_REPLY, PROBE, decode_reply};
use crate::platform::{disable_udp_connreset, local_ipv4_nets};

/// Longest wait for a reply before the loop looks at events, expiry and the stop flag.
const RECV_POLL: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeTargets {
    /// The directed broadcast address of every IPv4 interface that is up.
    Interfaces,
    /// Exact targets (tests).
    Fixed(Vec<SocketAddr>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserConfig {
    /// Probe port for `ProbeTargets::Interfaces`.
    pub discovery_port: u16,
    pub probe_every: Duration,
    pub targets: ProbeTargets,
    pub mdns: bool,
}

impl Default for BrowserConfig {
    fn default() -> Self {
        Self {
            discovery_port: 47800,
            probe_every: Duration::from_millis(1500),
            targets: ProbeTargets::Interfaces,
            mdns: true,
        }
    }
}

/// A sighting or a goodbye from a discovery source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiscoveryEvent {
    Seen(Advert, IpAddr, Source),
    Gone(DeviceId, Source),
}

pub type OnChange = Box<dyn FnMut(Vec<HostEntry>) + Send>;

#[derive(Debug)]
pub struct Browser {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    snapshot: Arc<Mutex<Vec<HostEntry>>>,
}

impl Browser {
    pub fn start(cfg: BrowserConfig, on_change: OnChange) -> Result<Browser, DiscoveryError> {
        let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))?;
        socket.set_broadcast(true)?;
        socket.set_read_timeout(Some(RECV_POLL))?;
        if let Err(error) = disable_udp_connreset(&socket) {
            log::debug!("SIO_UDP_CONNRESET not disabled: {error}");
        }
        let (_events_tx, events) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let snapshot = Arc::new(Mutex::new(Vec::new()));
        let mut worker = Worker {
            cfg,
            socket,
            events,
            on_change,
            snapshot: Arc::clone(&snapshot),
            list: HostList::new(),
            local: Vec::new(),
            had_targets: None,
        };
        let flag = Arc::clone(&stop);
        let thread = thread::Builder::new()
            .name("discovery-browse".into())
            .spawn(move || worker.run(&flag))?;
        Ok(Browser {
            stop,
            thread: Some(thread),
            snapshot,
        })
    }

    /// The hosts as of the last change.
    pub fn snapshot(&self) -> Vec<HostEntry> {
        self.snapshot
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub fn stop(self) {}
}

impl Drop for Browser {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.thread.take()
            && handle.join().is_err()
        {
            log::error!("discovery browser panicked");
        }
    }
}

struct Worker {
    cfg: BrowserConfig,
    socket: UdpSocket,
    events: Receiver<DiscoveryEvent>,
    on_change: OnChange,
    snapshot: Arc<Mutex<Vec<HostEntry>>>,
    list: HostList,
    local: Vec<LocalNet>,
    /// Last "any probe target?" answer, so the state is logged once per change.
    had_targets: Option<bool>,
}

impl Worker {
    fn run(&mut self, stop: &AtomicBool) {
        let mut next_probe = Instant::now();
        let mut buf = [0u8; MAX_REPLY + 1];
        while !stop.load(Ordering::Relaxed) {
            let mut changed = false;
            if Instant::now() >= next_probe {
                self.probe();
                next_probe = Instant::now() + self.cfg.probe_every;
            }
            match self.socket.recv_from(&mut buf) {
                Ok((len, src)) => {
                    if let Some(advert) = buf.get(..len).and_then(decode_reply) {
                        changed |=
                            self.list
                                .seen(&advert, src.ip(), Source::Broadcast, Instant::now());
                    }
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::ConnectionReset
                    ) => {}
                Err(error) => log::debug!("discovery receive failed: {error}"),
            }
            while let Ok(event) = self.events.try_recv() {
                changed |= match event {
                    DiscoveryEvent::Seen(advert, ip, source) => {
                        self.list.seen(&advert, ip, source, Instant::now())
                    }
                    DiscoveryEvent::Gone(id, source) => self.list.gone(&id, source),
                };
            }
            changed |= self.list.expire(Instant::now());
            if changed {
                self.publish();
            }
        }
    }

    fn probe(&mut self) {
        let targets = self.targets();
        let has = !targets.is_empty();
        if self.had_targets != Some(has) {
            if has {
                log::info!("discovery probing {} target(s)", targets.len());
            } else {
                log::info!("discovery has no IPv4 network to probe");
            }
            self.had_targets = Some(has);
        }
        for target in targets {
            if let Err(error) = self.socket.send_to(&PROBE, target) {
                log::debug!("discovery probe to {target} failed: {error}");
            }
        }
    }

    fn targets(&mut self) -> Vec<SocketAddr> {
        match &self.cfg.targets {
            ProbeTargets::Fixed(targets) => targets.clone(),
            ProbeTargets::Interfaces => {
                self.local = local_ipv4_nets().unwrap_or_else(|error| {
                    log::debug!("interface list unavailable: {error}");
                    Vec::new()
                });
                self.local
                    .iter()
                    .filter_map(LocalNet::broadcast)
                    .map(|ip| SocketAddr::new(IpAddr::V4(ip), self.cfg.discovery_port))
                    .collect()
            }
        }
    }

    fn publish(&mut self) {
        let entries = self.list.entries(&self.local);
        *self.snapshot.lock().unwrap_or_else(PoisonError::into_inner) = entries.clone();
        (self.on_change)(entries);
    }
}

#[cfg(test)]
#[path = "browser_tests.rs"]
mod tests;
