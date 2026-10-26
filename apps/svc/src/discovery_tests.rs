use mixlink_core::config::{HostConfig, NetConfig};
use mixlink_core::ids::DeviceId;
use mixlink_net::discovery::CAPS_V1;
use mixlink_proto::header::WIRE_VERSION;

use super::*;

#[test]
fn advert_from_host_config() {
    let config = HostConfig {
        name: "Living room".to_owned(),
        ..HostConfig::default()
    };
    let id = DeviceId::from_bytes([3; 16]);
    let advert = advert(&config, id);
    assert_eq!(advert.id, id);
    assert_eq!(advert.name, "Living room");
    assert_eq!(advert.port, 47801);
    assert_eq!(advert.ver, WIRE_VERSION);
    assert_eq!(advert.caps, CAPS_V1);
}

#[test]
fn settings_use_discovery_port() {
    let config = HostConfig {
        net: NetConfig {
            discovery_port: 50000,
            ..NetConfig::default()
        },
        ..HostConfig::default()
    };
    let settings = settings(&config, DeviceId::from_bytes([1; 16]));
    assert_eq!(settings.discovery_port, 50000);
}
