use std::net::Ipv4Addr;

use super::*;

#[test]
fn contains_same_subnet_only() {
    let net = LocalNet {
        addr: Ipv4Addr::new(192, 168, 1, 10),
        prefix: 24,
    };
    assert!(net.contains(Ipv4Addr::new(192, 168, 1, 77)));
    assert!(!net.contains(Ipv4Addr::new(192, 168, 2, 1)));
}

#[test]
fn prefix_zero_contains_everything() {
    let net = LocalNet {
        addr: Ipv4Addr::new(10, 0, 0, 1),
        prefix: 0,
    };
    assert!(net.contains(Ipv4Addr::new(8, 8, 8, 8)));
}

#[test]
fn broadcast_of_slash_8() {
    let net = LocalNet {
        addr: Ipv4Addr::new(10, 0, 0, 5),
        prefix: 8,
    };
    assert_eq!(net.broadcast(), Some(Ipv4Addr::new(10, 255, 255, 255)));
}

#[test]
fn no_broadcast_for_slash_31_and_32() {
    for prefix in [31, 32] {
        let net = LocalNet {
            addr: Ipv4Addr::new(10, 0, 0, 5),
            prefix,
        };
        assert_eq!(net.broadcast(), None);
    }
}
