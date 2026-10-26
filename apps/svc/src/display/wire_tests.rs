use super::*;

fn u32_at(buf: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([buf[at], buf[at + 1], buf[at + 2], buf[at + 3]])
}

fn mode(width: u32, height: u32, refresh_mhz: u32) -> WireMode {
    WireMode {
        width,
        height,
        refresh_mhz,
    }
}

fn request(edid_len: usize, modes: usize, preferred: u32) -> AddMonitor {
    AddMonitor {
        container: [7; 16],
        edid: vec![0xAB; edid_len],
        modes: (0..modes)
            .map(|i| mode(2560 - u32::try_from(i).unwrap_or(0), 1440, 120_000))
            .collect(),
        preferred,
    }
}

#[test]
fn add_layout_offsets() {
    let buf = request(128, 2, 1).encode().unwrap();
    assert_eq!(buf.len(), ADD_SIZE);
    assert_eq!(u32_at(&buf, 0), WIRE_VERSION);
    assert_eq!(u32_at(&buf, 4), 0);
    assert_eq!(&buf[8..24], &[7; 16]);
    assert_eq!(u32_at(&buf, 24), 128);
    assert_eq!(u32_at(&buf, 28), 2);
    assert_eq!(u32_at(&buf, 32), 1);
    assert_eq!(buf[36], 0xAB);
    assert_eq!(buf[36 + 128], 0);
    assert_eq!(u32_at(&buf, 548), 2560);
    assert_eq!(u32_at(&buf, 552), 1440);
    assert_eq!(u32_at(&buf, 556), 120_000);
    assert_eq!(u32_at(&buf, 560), 2559);
}

#[test]
fn add_rejects_bad_input() {
    assert_eq!(request(128, 0, 0).encode(), Err(WireError::NoModes));
    assert_eq!(request(128, 17, 0).encode(), Err(WireError::TooManyModes));
    assert_eq!(request(100, 1, 0).encode(), Err(WireError::EdidLength));
    assert_eq!(request(640, 1, 0).encode(), Err(WireError::EdidLength));
    assert_eq!(request(0, 1, 0).encode(), Err(WireError::EdidLength));
    assert_eq!(request(128, 2, 2).encode(), Err(WireError::BadPreferred));
}

#[test]
fn remove_and_query_requests() {
    let remove = encode_remove(9);
    assert_eq!(remove.len(), REMOVE_SIZE);
    assert_eq!(u32_at(&remove, 0), WIRE_VERSION);
    assert_eq!(u32_at(&remove, 4), 0);
    assert_eq!(u32_at(&remove, 8), 9);
    let query = encode_query();
    assert_eq!(query.len(), QUERY_IN_SIZE);
    assert_eq!(u32_at(&query, 0), WIRE_VERSION);
}

fn query_buf(count: u32) -> Vec<u8> {
    let mut buf = vec![0u8; QUERY_SIZE];
    buf[0..4].copy_from_slice(&WIRE_VERSION.to_le_bytes());
    buf[8..12].copy_from_slice(&0x0104_u32.to_le_bytes());
    buf[12..16].copy_from_slice(&0x1234_u32.to_le_bytes());
    buf[16..20].copy_from_slice(&(-1_i32).to_le_bytes());
    buf[20..24].copy_from_slice(&count.to_le_bytes());
    for (slot, (id, connector)) in [(1_u32, 0_u32), (3, 2)].into_iter().enumerate() {
        let at = 24 + slot * 20;
        for (i, value) in [id, connector, 1920, 1080, 60_000].into_iter().enumerate() {
            buf[at + i * 4..at + i * 4 + 4].copy_from_slice(&value.to_le_bytes());
        }
    }
    buf
}

#[test]
fn query_roundtrip_two_monitors() {
    let reply = decode_query(&query_buf(2)).unwrap();
    assert_eq!(reply.iddcx_version, 0x0104);
    assert_eq!(reply.render_luid, (0x1234, -1));
    assert_eq!(
        reply.monitors,
        vec![
            MonitorInfo {
                id: 1,
                connector: 0,
                mode: mode(1920, 1080, 60_000)
            },
            MonitorInfo {
                id: 3,
                connector: 2,
                mode: mode(1920, 1080, 60_000)
            },
        ]
    );
}

#[test]
fn query_rejects_short_version_count() {
    assert_eq!(decode_query(&query_buf(2)[..50]), Err(WireError::Short));
    let mut wrong_version = query_buf(2);
    wrong_version[0] = 2;
    assert_eq!(decode_query(&wrong_version), Err(WireError::Version));
    assert_eq!(decode_query(&query_buf(5)), Err(WireError::Count));
}

#[test]
fn add_reply_reads_id() {
    assert_eq!(decode_add_reply(&5_u32.to_le_bytes()), Ok(5));
    assert_eq!(decode_add_reply(&[1, 2]), Err(WireError::Short));
}

/// `CTL_CODE(FILE_DEVICE_UNKNOWN, function, METHOD_BUFFERED, FILE_ANY_ACCESS)`.
fn ctl_code(function: u32) -> u32 {
    (0x22 << 16) | (function << 2)
}

#[test]
fn ioctl_codes_match_ctl_code() {
    assert_eq!(IOCTL_ADD_MONITOR, ctl_code(0x900));
    assert_eq!(IOCTL_REMOVE_MONITOR, ctl_code(0x901));
    assert_eq!(IOCTL_QUERY, ctl_code(0x902));
}
