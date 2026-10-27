use std::net::{IpAddr, Ipv4Addr};
use std::time::{Duration, Instant};

use mixlink_core::ids::DeviceId;

use super::*;
use crate::discovery::advert::CAPS_V1;

fn advert(id: u8, name: &str) -> Advert {
    Advert {
        id: DeviceId::from_bytes([id; 16]),
        name: name.to_owned(),
        ver: 1,
        caps: CAPS_V1.to_owned(),
        port: 47801,
    }
}

fn ip(a: u8, b: u8, c: u8, d: u8) -> IpAddr {
    IpAddr::V4(Ipv4Addr::new(a, b, c, d))
}

const LAN: IpAddr = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 9));

#[test]
fn first_sighting_changes() {
    let mut list = HostList::new();
    assert!(list.seen(&advert(1, "A"), LAN, Source::Broadcast, Instant::now()));
    assert_eq!(list.entries(&[]).len(), 1);
}

#[test]
fn same_report_again_no_change() {
    let t0 = Instant::now();
    let mut list = HostList::new();
    list.seen(&advert(1, "A"), LAN, Source::Broadcast, t0);
    assert!(!list.seen(
        &advert(1, "A"),
        LAN,
        Source::Broadcast,
        t0 + Duration::from_secs(1)
    ));
    assert!(!list.seen(
        &advert(1, "A"),
        LAN,
        Source::Mdns,
        t0 + Duration::from_secs(1)
    ));
}

#[test]
fn expires_after_5s() {
    let t0 = Instant::now();
    let mut list = HostList::new();
    list.seen(&advert(1, "A"), LAN, Source::Broadcast, t0);
    assert!(!list.expire(t0 + Duration::from_millis(4900)));
    assert!(list.expire(t0 + EXPIRY));
    assert!(list.entries(&[]).is_empty());
}

#[test]
fn newer_sighting_keeps_entry() {
    let t0 = Instant::now();
    let mut list = HostList::new();
    list.seen(&advert(1, "A"), LAN, Source::Broadcast, t0);
    list.seen(
        &advert(1, "A"),
        LAN,
        Source::Mdns,
        t0 + Duration::from_secs(4),
    );
    assert!(!list.expire(t0 + Duration::from_secs(6)));
    assert_eq!(list.entries(&[]).len(), 1);
}

#[test]
fn mdns_gone_drops_only_mdns_source() {
    let t0 = Instant::now();
    let mut list = HostList::new();
    let a = advert(1, "A");
    list.seen(&a, LAN, Source::Broadcast, t0);
    list.seen(&a, LAN, Source::Mdns, t0);
    assert!(!list.gone(&a.id, Source::Mdns));
    assert_eq!(list.entries(&[]).len(), 1);
}

#[test]
fn gone_last_source_removes() {
    let t0 = Instant::now();
    let mut list = HostList::new();
    let a = advert(1, "A");
    list.seen(&a, LAN, Source::Mdns, t0);
    assert!(list.gone(&a.id, Source::Mdns));
    assert!(list.entries(&[]).is_empty());
}

#[test]
fn same_name_different_id_two_rows() {
    let t0 = Instant::now();
    let mut list = HostList::new();
    list.seen(&advert(1, "MixLink Host"), LAN, Source::Broadcast, t0);
    list.seen(
        &advert(2, "MixLink Host"),
        ip(192, 168, 1, 10),
        Source::Broadcast,
        t0,
    );
    let entries = list.entries(&[]);
    assert_eq!(entries.len(), 2);
    assert_ne!(entries[0].id, entries[1].id);
}

#[test]
fn rename_takes_newest() {
    let t0 = Instant::now();
    let mut list = HostList::new();
    list.seen(&advert(1, "Old"), LAN, Source::Broadcast, t0);
    let mut renamed = advert(1, "New");
    renamed.port = 50000;
    assert!(list.seen(&renamed, LAN, Source::Mdns, t0 + Duration::from_secs(1)));
    let entries = list.entries(&[]);
    assert_eq!(entries[0].name, "New");
    assert_eq!(entries[0].port, 50000);
}

#[test]
fn entries_sorted_by_name() {
    let t0 = Instant::now();
    let mut list = HostList::new();
    list.seen(&advert(1, "Zeta"), LAN, Source::Broadcast, t0);
    list.seen(&advert(2, "Alpha"), LAN, Source::Broadcast, t0);
    let names: Vec<_> = list.entries(&[]).into_iter().map(|e| e.name).collect();
    assert_eq!(names, ["Alpha", "Zeta"]);
}

#[test]
fn address_prefers_local_subnet() {
    let t0 = Instant::now();
    let mut list = HostList::new();
    let a = advert(1, "A");
    list.seen(&a, ip(192, 168, 1, 9), Source::Broadcast, t0);
    list.seen(
        &a,
        ip(10, 0, 0, 9),
        Source::Mdns,
        t0 + Duration::from_secs(1),
    );
    let local = [LocalNet {
        addr: Ipv4Addr::new(192, 168, 1, 50),
        prefix: 24,
    }];
    assert_eq!(list.entries(&local)[0].address, ip(192, 168, 1, 9));
}

#[test]
fn address_newest_when_none_local() {
    let t0 = Instant::now();
    let mut list = HostList::new();
    let a = advert(1, "A");
    list.seen(&a, ip(192, 168, 1, 9), Source::Broadcast, t0);
    list.seen(
        &a,
        ip(10, 0, 0, 9),
        Source::Mdns,
        t0 + Duration::from_secs(1),
    );
    assert_eq!(list.entries(&[])[0].address, ip(10, 0, 0, 9));
}

#[test]
fn stale_address_expires_alone() {
    let t0 = Instant::now();
    let mut list = HostList::new();
    let a = advert(1, "A");
    list.seen(&a, ip(10, 0, 0, 9), Source::Mdns, t0);
    list.seen(&a, LAN, Source::Broadcast, t0 + Duration::from_secs(4));
    list.expire(t0 + Duration::from_secs(6));
    assert_eq!(list.entries(&[])[0].address, LAN);
}
