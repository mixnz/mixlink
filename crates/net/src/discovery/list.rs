//! The client's host list (spec 0017): mDNS and broadcast sightings merged by host id.
//! Pure: callers pass the time, so tests use a fake clock.

use std::collections::HashMap;
use std::net::IpAddr;
use std::time::{Duration, Instant};

use mixlink_core::ids::DeviceId;

use crate::discovery::advert::Advert;
use crate::discovery::local_net::LocalNet;

/// A host no source has seen for this long is removed.
pub const EXPIRY: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Source {
    Mdns,
    Broadcast,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostEntry {
    pub id: DeviceId,
    pub name: String,
    pub ver: u8,
    pub caps: String,
    pub port: u16,
    /// The address to connect to (see `HostList::entries`).
    pub address: IpAddr,
}

#[derive(Debug)]
struct Known {
    advert: Advert,
    /// Last sighting per address and source.
    addrs: HashMap<IpAddr, HashMap<Source, Instant>>,
}

impl Known {
    fn last_seen(sources: &HashMap<Source, Instant>) -> Option<Instant> {
        sources.values().max().copied()
    }

    /// Newest address on a local subnet, else the newest one.
    fn address(&self, local: &[LocalNet]) -> Option<IpAddr> {
        let is_local = |ip: &IpAddr| match ip {
            IpAddr::V4(v4) => local.iter().any(|net| net.contains(*v4)),
            IpAddr::V6(_) => false,
        };
        let newest = |only_local: bool| {
            self.addrs
                .iter()
                .filter(|(ip, _)| !only_local || is_local(ip))
                .filter_map(|(ip, sources)| Some((Known::last_seen(sources)?, *ip)))
                .max()
                .map(|(_, ip)| ip)
        };
        newest(true).or_else(|| newest(false))
    }
}

#[derive(Debug, Default)]
pub struct HostList {
    hosts: HashMap<DeviceId, Known>,
}

impl HostList {
    pub fn new() -> HostList {
        HostList::default()
    }

    /// Records a sighting; `true` when the host, its fields or its address set changed.
    pub fn seen(&mut self, advert: &Advert, addr: IpAddr, source: Source, now: Instant) -> bool {
        let Some(known) = self.hosts.get_mut(&advert.id) else {
            let addrs = HashMap::from([(addr, HashMap::from([(source, now)]))]);
            let known = Known {
                advert: advert.clone(),
                addrs,
            };
            self.hosts.insert(advert.id, known);
            return true;
        };
        let mut changed = false;
        if known.advert != *advert {
            known.advert = advert.clone();
            changed = true;
        }
        let sources = known.addrs.entry(addr).or_insert_with(|| {
            changed = true;
            HashMap::new()
        });
        sources.insert(source, now);
        changed
    }

    /// Forgets every sighting of `id` by `source` (an mDNS goodbye). `true` when the list changed.
    pub fn gone(&mut self, id: &DeviceId, source: Source) -> bool {
        self.prune(|host_id, src, _| host_id == id && src == source)
    }

    /// Drops sightings older than `EXPIRY`. `true` when the list changed.
    pub fn expire(&mut self, now: Instant) -> bool {
        self.prune(|_, _, at| now.saturating_duration_since(at) >= EXPIRY)
    }

    /// Current hosts sorted by name, then id; the address prefers a subnet in `local`.
    pub fn entries(&self, local: &[LocalNet]) -> Vec<HostEntry> {
        let mut entries: Vec<HostEntry> = self
            .hosts
            .values()
            .filter_map(|known| {
                let advert = &known.advert;
                Some(HostEntry {
                    id: advert.id,
                    name: advert.name.clone(),
                    ver: advert.ver,
                    caps: advert.caps.clone(),
                    port: advert.port,
                    address: known.address(local)?,
                })
            })
            .collect();
        entries.sort_by(|a, b| {
            (a.name.as_str(), a.id.as_bytes()).cmp(&(b.name.as_str(), b.id.as_bytes()))
        });
        entries
    }

    /// Removes sightings matching `drop`, then empty addresses and hosts. `true` when an address
    /// or a host went away.
    fn prune(&mut self, drop: impl Fn(&DeviceId, Source, Instant) -> bool) -> bool {
        let mut changed = false;
        self.hosts.retain(|id, known| {
            known.addrs.retain(|_, sources| {
                sources.retain(|source, at| !drop(id, *source, *at));
                let keep = !sources.is_empty();
                changed |= !keep;
                keep
            });
            !known.addrs.is_empty()
        });
        changed
    }
}

#[cfg(test)]
#[path = "list_tests.rs"]
mod tests;
