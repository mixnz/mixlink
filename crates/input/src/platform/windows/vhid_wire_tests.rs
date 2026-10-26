use super::*;

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
    let reply = decode_query(&query_bytes(1, 7, 1, [8, 7, 5, 13])).unwrap();
    assert_eq!(
        reply,
        QueryReply {
            version: 1,
            dropped: 7,
            feeder_held: true,
            sizes: [8, 7, 5, 13],
        }
    );
    let free = decode_query(&query_bytes(1, 0, 0, [8, 7, 5, 13])).unwrap();
    assert!(!free.feeder_held);
}

#[test]
fn query_rejects_short_and_version() {
    let full = query_bytes(1, 0, 0, [8, 7, 5, 13]);
    assert_eq!(decode_query(&full[..15]), Err(WireError::Short));
    assert_eq!(
        decode_query(&query_bytes(2, 0, 0, [8, 7, 5, 13])),
        Err(WireError::Version)
    );
}

#[test]
fn ioctl_codes() {
    assert_eq!(IOCTL_FEED, (0x22 << 16) | (0x900 << 2));
    assert_eq!(IOCTL_QUERY, (0x22 << 16) | (0x901 << 2));
}
