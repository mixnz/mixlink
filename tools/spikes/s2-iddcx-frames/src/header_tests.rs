use super::*;
use std::mem::{offset_of, size_of};

fn valid() -> S2Header {
    S2Header {
        magic: MAGIC,
        version: VERSION,
        slot_count: SLOT_COUNT as u32,
        ..S2Header::zeroed()
    }
}

#[test]
fn header_v2_layout() {
    assert_eq!(size_of::<S2Slot>(), 40);
    assert_eq!(size_of::<S2Header>(), 272);
    assert_eq!(offset_of!(S2Header, texture_handles), 40);
    assert_eq!(offset_of!(S2Header, latest_seq), 80);
    assert_eq!(offset_of!(S2Header, slots), 88);
    assert_eq!(offset_of!(S2Header, levers_requested), 248);
    assert_eq!(offset_of!(S2Header, lever_hr), 256);
}

#[test]
fn validate_accepts_valid_header() {
    assert_eq!(validate(&valid()), Ok(()));
}

#[test]
fn validate_rejects_bad_magic() {
    let h = S2Header {
        magic: 0,
        ..valid()
    };
    assert_eq!(validate(&h), Err(HeaderError::BadMagic));
}

#[test]
fn validate_rejects_bad_version() {
    let h = S2Header {
        version: 3,
        ..valid()
    };
    assert_eq!(validate(&h), Err(HeaderError::BadVersion(3)));
}

#[test]
fn validate_rejects_v1() {
    let h = S2Header {
        version: 1,
        ..valid()
    };
    assert_eq!(validate(&h), Err(HeaderError::BadVersion(1)));
}

#[test]
fn validate_rejects_bad_slot_count() {
    let h = S2Header {
        slot_count: 3,
        ..valid()
    };
    assert_eq!(validate(&h), Err(HeaderError::BadSlotCount(3)));
}

#[test]
fn generation_change_detected() {
    let mut watch = GenerationWatch::default();
    assert!(watch.changed(1));
    assert!(!watch.changed(1));
    assert!(watch.changed(2));
}
