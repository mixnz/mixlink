use super::*;
use crate::hid::gamepad::PadState;
use crate::hid::keyboard::KeyboardState;

const CURRENT: [u8; 4] = [16, 7, 5, 15];

fn query_bytes(version: u32, dropped: u32, flags: u32, sizes: [u8; 4]) -> [u8; QUERY_SIZE] {
    let mut out = [0u8; QUERY_SIZE];
    out[0..4].copy_from_slice(&version.to_le_bytes());
    out[4..8].copy_from_slice(&dropped.to_le_bytes());
    out[8..12].copy_from_slice(&flags.to_le_bytes());
    out[12..16].copy_from_slice(&sizes);
    out
}

#[test]
fn query_in_is_version() {
    assert_eq!(encode_query(), [1, 0, 0, 0]);
}

#[test]
fn query_decodes_fields() {
    let reply = decode_query(&query_bytes(1, 7, 1, CURRENT)).unwrap();
    assert_eq!(
        reply,
        QueryReply {
            version: 1,
            dropped: 7,
            feeder_held: true,
            sizes: CURRENT,
        }
    );
    let free = decode_query(&query_bytes(1, 0, 0, CURRENT)).unwrap();
    assert!(!free.feeder_held);
}

#[test]
fn query_rejects_short_and_version() {
    let full = query_bytes(1, 0, 0, CURRENT);
    assert_eq!(decode_query(&full[..15]), Err(WireError::Short));
    assert_eq!(
        decode_query(&query_bytes(2, 0, 0, CURRENT)),
        Err(WireError::Version)
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
    assert_eq!(&bytes[0..4], &[1, 0, 0, 0]);
    assert_eq!(bytes[4], 1);
    assert_eq!(bytes[5], 16);
    assert_eq!(&bytes[6..8], &[0, 0]);
    assert_eq!(&bytes[8..24], report.payload());
}

#[test]
fn feed_pad_zero_pads_tail() {
    let pad = PadState {
        buttons: 0xFFFF,
        lt: 65_535,
        rt: 65_535,
        ..PadState::NEUTRAL
    };
    let bytes = encode_feed(&pad.report());
    assert_eq!(bytes[4], 4);
    assert_eq!(bytes[5], 15);
    assert_eq!(bytes[23], 0);
}

#[test]
fn query_rejects_size_mismatch() {
    assert_eq!(
        decode_query(&query_bytes(1, 0, 0, [8, 7, 5, 13])),
        Err(WireError::Sizes)
    );
}

#[test]
fn query_accepts_current_sizes() {
    assert_eq!(
        decode_query(&query_bytes(1, 0, 0, CURRENT)).unwrap().sizes,
        CURRENT
    );
}
