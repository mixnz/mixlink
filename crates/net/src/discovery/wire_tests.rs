use mixlink_core::ids::DeviceId;

use super::*;
use crate::discovery::advert::CAPS_V1;

fn advert(name: &str) -> Advert {
    Advert {
        id: DeviceId::from_bytes([9; 16]),
        name: name.to_owned(),
        ver: 1,
        caps: CAPS_V1.to_owned(),
        port: 47801,
    }
}

fn reply(advert: &Advert) -> Vec<u8> {
    let mut out = Vec::new();
    encode_reply(advert, &mut out).unwrap();
    out
}

#[test]
fn probe_is_probe() {
    assert!(is_probe(&PROBE));
    assert_eq!(&PROBE, b"MXLD\x01");
}

#[test]
fn not_probes() {
    assert!(!is_probe(b"MXLD"));
    assert!(!is_probe(b"MXLD\x02"));
    assert!(!is_probe(b"XXLD\x01"));
    assert!(!is_probe(b"MXLD\x01x"));
    assert!(!is_probe(b""));
}

#[test]
fn reply_round_trips() {
    let a = advert("Desk PC");
    let bytes = reply(&a);
    assert!(bytes.starts_with(&PROBE));
    assert!(bytes.len() <= MAX_REPLY);
    assert_eq!(decode_reply(&bytes), Some(a));
}

#[test]
fn reply_round_trips_unicode_name() {
    let a = advert(&format!("{}abc", "ệ".repeat(20)));
    assert_eq!(decode_reply(&reply(&a)), Some(a));
}

#[test]
fn encode_reply_rejects_oversize() {
    let mut a = advert("x");
    a.caps = "c".repeat(MAX_REPLY);
    let mut out = Vec::new();
    assert!(matches!(
        encode_reply(&a, &mut out),
        Err(DiscoveryError::ReplyTooLong { .. })
    ));
}

#[test]
fn decode_reply_rejects_bad_input() {
    assert_eq!(decode_reply(&PROBE), None);
    assert_eq!(decode_reply(b"MXLD\x01{not json"), None);
    let mut wrong_version = reply(&advert("x"));
    wrong_version[4] = 2;
    assert_eq!(decode_reply(&wrong_version), None);
    assert_eq!(decode_reply(b"MXL"), None);
}

#[test]
fn decode_reply_rejects_long_name() {
    let mut a = advert("x");
    a.name = "a".repeat(64);
    let mut out = PROBE.to_vec();
    out.extend_from_slice(serde_json::to_string(&a).unwrap().as_bytes());
    assert_eq!(decode_reply(&out), None);
}

#[test]
fn fuzz_decode_never_panics() {
    let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    let valid = reply(&advert("fuzz"));
    for i in 0..10_000 {
        let len = (next() % 1300) as usize;
        let mut buf: Vec<u8> = (0..len).map(|_| next() as u8).collect();
        // Every other buffer starts like a real reply so the JSON parser sees noise too.
        if i % 2 == 0 && buf.len() >= 5 {
            buf[..5].copy_from_slice(&valid[..5]);
        }
        let _ = decode_reply(&buf);
        let _ = is_probe(&buf);
    }
}
