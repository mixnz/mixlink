use std::net::UdpSocket;

use mixlink_core::ids::DeviceId;

use super::*;
use crate::discovery::advert::CAPS_V1;

fn advert() -> Advert {
    Advert {
        id: DeviceId::from_bytes([5; 16]),
        name: "Test".to_owned(),
        ver: 1,
        caps: CAPS_V1.to_owned(),
        port: 47801,
    }
}

#[test]
fn port_zero_gets_a_port() {
    let cfg = AdvertiserConfig {
        discovery_port: 0,
        mdns: false,
    };
    let adv = Advertiser::start(advert(), cfg).unwrap();
    assert!(adv.local_port().is_some_and(|port| port != 0));
}

#[test]
fn busy_port_is_nothing_started() {
    let taken = UdpSocket::bind("0.0.0.0:0").unwrap();
    let cfg = AdvertiserConfig {
        discovery_port: taken.local_addr().unwrap().port(),
        mdns: false,
    };
    assert!(matches!(
        Advertiser::start(advert(), cfg),
        Err(DiscoveryError::NothingStarted)
    ));
}
