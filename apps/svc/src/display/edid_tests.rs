use super::*;

const ID: [u8; 16] = [
    0x01, 0x23, 0x45, 0x67, 0x89, 0xAB, 0xCD, 0xEF, 0x10, 0x32, 0x54, 0x76, 0x98, 0xBA, 0xDC, 0xFE,
];

fn mode(width: u32, height: u32, refresh_mhz: u32, dpi: u16) -> DisplayInfo {
    DisplayInfo {
        width,
        height,
        refresh_mhz,
        hdr: false,
        dpi,
    }
}

fn client() -> Edid {
    build(
        &DeviceId::from_bytes(ID),
        PRODUCT_CLIENT,
        &mode(1920, 1080, 60_000, 96),
    )
}

/// The 18-byte display descriptor with this tag (bytes 54..126), as written.
fn descriptor(edid: &Edid, tag: u8) -> Option<[u8; 18]> {
    (0..4)
        .map(|i| 54 + i * 18)
        .find(|&at| edid.base[at..at + 3] == [0, 0, 0] && edid.base[at + 3] == tag)
        .map(|at| {
            let mut out = [0u8; 18];
            out.copy_from_slice(&edid.base[at..at + 18]);
            out
        })
}

#[test]
fn header_and_checksum() {
    let edid = client();
    assert_eq!(
        &edid.base[0..8],
        &[0, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0]
    );
    let sum: u32 = edid.base.iter().map(|&b| u32::from(b)).sum();
    assert_eq!(sum % 256, 0);
    assert_eq!(edid.base[126], 0);
    assert!(edid.extensions.is_empty());
    assert_eq!(edid.to_bytes().len(), 128);
}

#[test]
fn identity_fields() {
    let edid = client();
    assert_eq!(&edid.base[8..10], &MXL_MFG);
    assert_eq!(&edid.base[10..12], &[0x58, 0x4D]);
    assert_eq!(&edid.base[12..16], &ID[0..4]);
    assert_eq!(edid.base[17], 36);
    assert_eq!(&edid.base[18..20], &[1, 4]);
    let serial = descriptor(&edid, 0xFF).unwrap();
    assert_eq!(&serial[5..18], b"0123456789ab\n");
    let name = descriptor(&edid, 0xFC).unwrap();
    assert_eq!(&name[5..18], b"MixLink\n     ");
}

#[test]
fn headless_product_code() {
    let edid = build(&HEADLESS_ID, PRODUCT_HEADLESS, &HEADLESS_MODE);
    assert_eq!(&edid.base[10..12], &[0x59, 0x4D]);
    assert_eq!(HEADLESS_ID.as_bytes()[15], 1);
    assert_eq!(HEADLESS_MODE.width, 1920);
    assert_eq!(HEADLESS_MODE.refresh_mhz, 60_000);
}

#[test]
fn size_from_dpi() {
    let id = DeviceId::from_bytes(ID);
    let edid = build(&id, PRODUCT_CLIENT, &mode(2560, 1440, 120_000, 109));
    assert_eq!((edid.base[21], edid.base[22]), (60, 34));
    let edid = build(&id, PRODUCT_CLIENT, &mode(2560, 1440, 120_000, 0));
    assert_eq!((edid.base[21], edid.base[22]), (0, 0));
}

#[test]
fn same_id_same_bytes() {
    assert_eq!(client().to_bytes(), client().to_bytes());
}

#[test]
fn different_ids_different_serials() {
    let mut other = ID;
    other[0] ^= 0xFF;
    let a = client();
    let b = build(
        &DeviceId::from_bytes(other),
        PRODUCT_CLIENT,
        &mode(1920, 1080, 60_000, 96),
    );
    assert_ne!(&a.base[12..16], &b.base[12..16]);
    assert_ne!(descriptor(&a, 0xFF), descriptor(&b, 0xFF));
}

#[test]
fn dtd_matches_client_mode() {
    let edid = client();
    assert_eq!(&edid.base[54..56], &[0x1A, 0x36]);
    // Horizontal active 1920 = 0x780, blank 160 = 0x0A0.
    assert_eq!(edid.base[56], 0x80);
    assert_eq!(edid.base[57], 0xA0);
    assert_eq!(edid.base[58], 0x70);
    // Vertical active 1080 = 0x438, blank 31 = 0x01F.
    assert_eq!(edid.base[59], 0x38);
    assert_eq!(edid.base[60], 0x1F);
    assert_eq!(edid.base[61], 0x40);
}

#[test]
fn dtd_falls_back_above_clock_limit() {
    let edid = build(
        &DeviceId::from_bytes(ID),
        PRODUCT_CLIENT,
        &mode(3840, 2160, 144_000, 96),
    );
    assert_eq!(&edid.base[54..62], &client().base[54..62]);
}

#[test]
fn container_is_v4_guid() {
    let guid = container_id(&DeviceId::from_bytes(ID));
    assert_eq!(guid[6] >> 4, 4);
    assert_eq!(guid[8] >> 6, 0b10);
    assert_eq!(&guid[0..6], &ID[0..6]);
}
