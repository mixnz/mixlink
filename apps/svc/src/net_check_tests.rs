use std::time::{Duration, Instant};

use mixlink_core::config::NetConfig;

use super::*;

#[test]
fn local_ports_two() {
    assert_eq!(local_ports(&NetConfig::default()), "47800,47801");
}

#[test]
fn local_ports_same_once() {
    let net = NetConfig {
        discovery_port: 50000,
        session_port: 50000,
    };
    assert_eq!(local_ports(&net), "50000");
}

#[test]
fn public_check_caches_for_ttl() {
    let t0 = Instant::now();
    let mut check = PublicCheck::new(Duration::from_secs(5));
    let mut probes = 0;
    let mut probe = |answer: bool| {
        probes += 1;
        answer
    };
    assert!(check.get(t0, || probe(true)));
    assert!(check.get(t0 + Duration::from_secs(4), || probe(false)));
    assert!(!check.get(t0 + Duration::from_secs(5), || probe(false)));
    assert_eq!(probes, 2);
}

#[cfg(windows)]
#[test]
fn public_network_check_answers() {
    assert!(crate::platform::windows::network_category::any_public_network().is_ok());
}
