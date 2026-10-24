use super::*;

fn sample_device_id() -> DeviceId {
    let mut bytes = [0u8; 16];
    for (i, b) in bytes.iter_mut().enumerate() {
        *b = i as u8;
    }
    DeviceId::from_bytes(bytes)
}

#[test]
fn seq_next_wraps() {
    assert_eq!(Seq::new(u32::MAX).next(), Seq::new(0));
    assert_eq!(FrameId::new(u32::MAX).next(), FrameId::new(0));
}

#[test]
fn seq_newer_across_wrap() {
    assert!(Seq::new(0).is_newer_than(Seq::new(u32::MAX)));
    assert!(!Seq::new(u32::MAX).is_newer_than(Seq::new(0)));
    assert!(Seq::new(5).is_newer_than(Seq::new(4)));
    assert!(!Seq::new(4).is_newer_than(Seq::new(4)));
}

#[test]
fn frame_id_newer_across_wrap() {
    assert!(FrameId::new(0).is_newer_than(FrameId::new(u32::MAX)));
    assert!(!FrameId::new(u32::MAX).is_newer_than(FrameId::new(0)));
    assert!(FrameId::new(5).is_newer_than(FrameId::new(4)));
    assert!(!FrameId::new(4).is_newer_than(FrameId::new(4)));
}

#[test]
fn seq_half_range_is_not_newer() {
    let a = Seq::new(0);
    let b = Seq::new(1 << 31);
    assert!(!a.is_newer_than(b));
    assert!(!b.is_newer_than(a));
}

#[test]
fn device_id_display_round_trips() {
    let id = sample_device_id();
    let text = id.to_string();
    assert_eq!(text, "000102030405060708090a0b0c0d0e0f");
    assert_eq!(text.parse::<DeviceId>().unwrap(), id);
}

#[test]
fn device_id_accepts_uppercase() {
    let id: DeviceId = "000102030405060708090A0B0C0D0E0F".parse().unwrap();
    assert_eq!(id, sample_device_id());
}

#[test]
fn device_id_wrong_length_is_error() {
    let text = "0".repeat(31);
    assert_eq!(
        text.parse::<DeviceId>(),
        Err(IdParseError::Length { len: 31 })
    );
}

#[test]
fn device_id_non_hex_is_error() {
    let text = format!("0000g{}", "0".repeat(27));
    assert_eq!(
        text.parse::<DeviceId>(),
        Err(IdParseError::NotHex { index: 4 })
    );
}

#[test]
fn device_id_non_ascii_is_error() {
    let text = format!("{}é", "0".repeat(30));
    assert_eq!(text.len(), 32);
    assert_eq!(
        text.parse::<DeviceId>(),
        Err(IdParseError::NotHex { index: 30 })
    );
}

#[test]
fn device_id_serde_is_hex_string() {
    let id = sample_device_id();
    let json = serde_json::to_string(&id).unwrap();
    assert_eq!(json, "\"000102030405060708090a0b0c0d0e0f\"");
    assert_eq!(serde_json::from_str::<DeviceId>(&json).unwrap(), id);
    assert!(serde_json::from_str::<DeviceId>("\"zz\"").is_err());
}

#[test]
fn session_id_serde_is_number() {
    let id = SessionId::new(7);
    assert_eq!(serde_json::to_string(&id).unwrap(), "7");
    assert_eq!(serde_json::from_str::<SessionId>("7").unwrap(), id);
}
