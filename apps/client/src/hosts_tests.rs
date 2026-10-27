use std::net::{IpAddr, Ipv4Addr};

use mixlink_core::ids::DeviceId;
use mixlink_net::discovery::{CAPS_V1, HostEntry};
use mixlink_proto::header::WIRE_VERSION;

use super::*;

fn entry(id: u8, name: &str, ver: u8) -> HostEntry {
    HostEntry {
        id: DeviceId::from_bytes([id; 16]),
        name: name.to_owned(),
        ver,
        caps: CAPS_V1.to_owned(),
        port: 47801,
        address: IpAddr::V4(Ipv4Addr::new(192, 168, 1, id)),
    }
}

#[test]
fn rows_mark_incompatible_ver() {
    let rows = rows(
        &[
            entry(1, "Old", WIRE_VERSION + 1),
            entry(2, "Now", WIRE_VERSION),
        ],
        &[],
    );
    assert!(!rows[0].compatible);
    assert!(rows[1].compatible);
}

#[test]
fn rows_keep_order_and_format_address() {
    let rows = rows(
        &[entry(2, "B", WIRE_VERSION), entry(1, "A", WIRE_VERSION)],
        &[],
    );
    assert_eq!(rows[0].name, "B");
    assert_eq!(rows[0].address, "192.168.1.2");
    assert_eq!(rows[0].id, "02".repeat(16));
    assert_eq!(rows[0].port, 47801);
    assert_eq!(rows[1].name, "A");
}

#[test]
fn row_serializes_camel_case() {
    let json = serde_json::to_value(&rows(&[entry(1, "A", WIRE_VERSION)], &[])[0]).unwrap();
    assert_eq!(json["compatible"], true);
    assert_eq!(json["address"], "192.168.1.1");
}

#[test]
fn row_paired_from_known_hosts() {
    let known = [DeviceId::from_bytes([2; 16])];
    let rows = rows(
        &[entry(1, "A", WIRE_VERSION), entry(2, "B", WIRE_VERSION)],
        &known,
    );
    assert!(!rows[0].paired);
    assert!(rows[1].paired);
    let json = serde_json::to_value(&rows[1]).unwrap();
    assert_eq!(json["paired"], true);
}
