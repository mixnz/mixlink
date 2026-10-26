use super::*;
use crate::hid::keyboard::KeyboardState;
use crate::hid::xbox::PadState;

const CURRENT: [u8; 4] = [16, 7, 5, 16];

fn query_bytes(
    version: u32,
    dropped: u32,
    flags: u32,
    sizes: [u8; 4],
    pads: u8,
) -> [u8; QUERY_SIZE] {
    let mut out = [0u8; QUERY_SIZE];
    out[0..4].copy_from_slice(&version.to_le_bytes());
    out[4..8].copy_from_slice(&dropped.to_le_bytes());
    out[8..12].copy_from_slice(&flags.to_le_bytes());
    out[12..16].copy_from_slice(&sizes);
    out[16] = pads;
    out
}

#[test]
fn query_in_is_version() {
    assert_eq!(encode_query(), [2, 0, 0, 0]);
}

#[test]
fn query_v2_layout() {
    assert_eq!(QUERY_SIZE, 20);
    let reply = decode_query(&query_bytes(2, 7, 1, CURRENT, 0b0101)).unwrap();
    assert_eq!(
        reply,
        QueryReply {
            version: 2,
            dropped: 7,
            feeder_held: true,
            sizes: CURRENT,
            pads_present: 0b0101,
        }
    );
    let free = decode_query(&query_bytes(2, 0, 0, CURRENT, 0)).unwrap();
    assert!(!free.feeder_held);
}

#[test]
fn query_rejects_short_and_version() {
    let full = query_bytes(2, 0, 0, CURRENT, 0);
    assert_eq!(decode_query(&full[..19]), Err(WireError::Short));
    assert_eq!(
        decode_query(&query_bytes(1, 0, 0, CURRENT, 0)),
        Err(WireError::Version)
    );
}

#[test]
fn query_v2_rejects_size_mismatch() {
    assert_eq!(
        decode_query(&query_bytes(2, 0, 0, [16, 7, 5, 15], 0)),
        Err(WireError::Sizes)
    );
}

#[test]
fn ioctl_codes() {
    assert_eq!(IOCTL_FEED, (0x22 << 16) | (0x900 << 2));
    assert_eq!(IOCTL_QUERY, (0x22 << 16) | (0x901 << 2));
}

#[test]
fn feed_layout() {
    let mut kb = KeyboardState::default();
    kb.set(0x04, true);
    let report = kb.report();
    let bytes = encode_feed(&report);
    assert_eq!(bytes.len(), FEED_SIZE);
    assert_eq!(&bytes[0..4], &[2, 0, 0, 0]);
    assert_eq!(bytes[4], 1);
    assert_eq!(bytes[5], 16);
    assert_eq!(&bytes[6..8], &[0, 0]);
    assert_eq!(&bytes[8..24], report.payload());
}

#[test]
fn feed_pad_target_byte() {
    let bytes = encode_feed(&PadState::NEUTRAL.report(2));
    assert_eq!(bytes[4], 0x12);
    assert_eq!(bytes[5], 16);
    assert_eq!(&bytes[8..10], &[0x00, 0x80]);
}
