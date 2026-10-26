//! LAN discovery for the service (spec 0017): what the host advertises, and starting it.

use mixlink_core::config::HostConfig;
use mixlink_core::ids::DeviceId;
use mixlink_net::discovery::{Advert, Advertiser, AdvertiserConfig, CAPS_V1};
use mixlink_proto::header::WIRE_VERSION;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoverySettings {
    pub advert: Advert,
    pub discovery_port: u16,
}

pub fn advert(config: &HostConfig, id: DeviceId) -> Advert {
    Advert {
        id,
        name: config.name.clone(),
        ver: WIRE_VERSION,
        caps: CAPS_V1.to_owned(),
        port: config.net.session_port,
    }
}

pub fn settings(config: &HostConfig, id: DeviceId) -> DiscoverySettings {
    DiscoverySettings {
        advert: advert(config, id),
        discovery_port: config.net.discovery_port,
    }
}

/// The running advertiser, or `None` (logged): the service keeps running without discovery.
pub fn start(settings: DiscoverySettings) -> Option<Advertiser> {
    let cfg = AdvertiserConfig {
        discovery_port: settings.discovery_port,
        mdns: true,
    };
    match Advertiser::start(settings.advert, cfg) {
        Ok(advertiser) => Some(advertiser),
        Err(error) => {
            log::error!("LAN discovery not started: {error}");
            None
        }
    }
}

#[cfg(test)]
#[path = "discovery_tests.rs"]
mod tests;
