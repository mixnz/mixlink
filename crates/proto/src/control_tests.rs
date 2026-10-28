use super::*;

const DEVICE: &str = "000102030405060708090a0b0c0d0e0f";

fn display(width: u32, height: u32, refresh_mhz: u32) -> DisplayInfo {
    DisplayInfo {
        width,
        height,
        refresh_mhz,
        hdr: false,
        dpi: 96,
    }
}

fn hello() -> ControlMessage {
    ControlMessage::Hello {
        proto: 1,
        device: DEVICE.parse().unwrap(),
        name: "Laptop".to_owned(),
        displays: vec![display(2560, 1440, 120_000), display(1920, 1080, 59_940)],
        stream_display: 1,
        codecs: vec![CodecId::H264, CodecId::Hevc],
    }
}

fn hello_with(edit: impl FnOnce(&mut ControlMessage)) -> ControlMessage {
    let mut msg = hello();
    edit(&mut msg);
    msg
}

fn edit_hello(
    msg: &mut ControlMessage,
    f: impl FnOnce(&mut Vec<DisplayInfo>, &mut u8, &mut String),
) {
    if let ControlMessage::Hello {
        displays,
        stream_display,
        name,
        ..
    } = msg
    {
        f(displays, stream_display, name);
    }
}

fn round_trip(msg: &ControlMessage) -> ControlMessage {
    decode(&encode(msg).unwrap()).unwrap()
}

#[test]
fn every_message_round_trips() {
    let messages = [
        hello(),
        ControlMessage::Welcome {
            session: SessionId::new(42),
            mode: display(2560, 1440, 120_000),
            codec: CodecId::Hevc,
        },
        ControlMessage::Reject {
            reason: RejectReason::Version,
        },
        ControlMessage::Reject {
            reason: RejectReason::Busy,
        },
        ControlMessage::Reject {
            reason: RejectReason::NotPaired,
        },
        ControlMessage::Reject {
            reason: RejectReason::BadDisplay,
        },
        ControlMessage::Bye {
            reason: ByeReason::UserClosed,
        },
        ControlMessage::Bye {
            reason: ByeReason::Shutdown,
        },
        ControlMessage::Bye {
            reason: ByeReason::Timeout,
        },
    ];
    for msg in messages {
        assert_eq!(round_trip(&msg), msg);
    }
}

#[test]
fn hello_ntsc_refresh_is_exact() {
    let ControlMessage::Hello { displays, .. } = round_trip(&hello()) else {
        panic!("expected hello");
    };
    assert_eq!(displays[1].refresh_mhz, 59_940);
}

#[test]
fn unknown_field_is_ignored() {
    let json = format!(
        r#"{{"type":"Hello","proto":1,"device":"{DEVICE}","name":"PC","extra":1,
        "displays":[{{"width":1920,"height":1080,"refresh_mhz":60000,"hdr":true,"dpi":144,"new":0}}],
        "stream_display":0}}"#
    );
    assert!(matches!(
        decode(json.as_bytes()),
        Ok(ControlMessage::Hello { .. })
    ));
}

#[test]
fn garbage_is_parse_error() {
    assert!(matches!(
        decode(b"\x00\x01not json"),
        Err(ControlError::Parse(_))
    ));
    assert!(matches!(
        decode(br#"{"type":"Nope"}"#),
        Err(ControlError::Parse(_))
    ));
}

#[test]
fn no_displays_is_rejected() {
    let msg = hello_with(|m| {
        edit_hello(m, |d, s, _| {
            d.clear();
            *s = 0;
        })
    });
    assert!(matches!(encode(&msg), Err(ControlError::NoDisplays)));
}

#[test]
fn too_many_displays_is_rejected() {
    let msg = hello_with(|m| edit_hello(m, |d, _, _| d.resize(9, display(800, 600, 60_000))));
    assert!(matches!(
        encode(&msg),
        Err(ControlError::TooManyDisplays { count: 9 })
    ));
}

#[test]
fn stream_display_out_of_range_is_rejected() {
    let msg = hello_with(|m| edit_hello(m, |_, s, _| *s = 2));
    assert!(matches!(
        encode(&msg),
        Err(ControlError::StreamDisplay { index: 2, count: 2 })
    ));
}

#[test]
fn display_size_limits() {
    for (w, h) in [(0, 1080), (1920, 0), (16_385, 1080), (1920, 16_385)] {
        let msg = hello_with(|m| edit_hello(m, |d, _, _| d[0] = display(w, h, 60_000)));
        assert!(matches!(
            encode(&msg),
            Err(ControlError::DisplaySize { width, height }) if (width, height) == (w, h)
        ));
    }
    let max = hello_with(|m| edit_hello(m, |d, _, _| d[0] = display(16_384, 16_384, 60_000)));
    assert!(encode(&max).is_ok());
}

#[test]
fn refresh_limits() {
    for mhz in [0, 999, 1_000_001] {
        let msg = hello_with(|m| edit_hello(m, |d, _, _| d[0] = display(1920, 1080, mhz)));
        assert!(matches!(encode(&msg), Err(ControlError::Refresh { mhz: m }) if m == mhz));
    }
    for mhz in [1_000, 1_000_000] {
        let msg = hello_with(|m| edit_hello(m, |d, _, _| d[0] = display(1920, 1080, mhz)));
        assert!(encode(&msg).is_ok());
    }
}

#[test]
fn name_limits() {
    let empty = hello_with(|m| edit_hello(m, |_, _, n| *n = "  ".to_owned()));
    assert!(matches!(encode(&empty), Err(ControlError::EmptyName)));
    let long = hello_with(|m| edit_hello(m, |_, _, n| *n = "a".repeat(64)));
    assert!(matches!(
        encode(&long),
        Err(ControlError::NameTooLong { len: 64 })
    ));
    let max = hello_with(|m| edit_hello(m, |_, _, n| *n = "a".repeat(63)));
    assert!(encode(&max).is_ok());
}

#[test]
fn decode_validates_too() {
    let json = format!(
        r#"{{"type":"Hello","proto":1,"device":"{DEVICE}","name":"PC","displays":[],"stream_display":0}}"#
    );
    assert!(matches!(
        decode(json.as_bytes()),
        Err(ControlError::NoDisplays)
    ));
}

#[test]
fn welcome_mode_is_validated() {
    let msg = ControlMessage::Welcome {
        session: SessionId::new(1),
        mode: display(1920, 1080, 0),
        codec: CodecId::H264,
    };
    assert!(matches!(
        encode(&msg),
        Err(ControlError::Refresh { mhz: 0 })
    ));
}

fn device() -> DeviceId {
    DEVICE.parse().unwrap()
}

fn pair_messages() -> Vec<ControlMessage> {
    let key = PublicKey([9; 32]);
    vec![
        ControlMessage::PairRequest {
            proto: 1,
            device: device(),
            host: device(),
            name: "Phòng khách".to_owned(),
            key,
        },
        ControlMessage::PairPrompt { expires_s: 120 },
        ControlMessage::PairStart {
            device: device(),
            spake: Blob(vec![1; SPAKE_LEN]),
        },
        ControlMessage::PairReply {
            spake: Blob(vec![2; SPAKE_LEN]),
            key,
            mac: Blob(vec![3; MAC_LEN]),
        },
        ControlMessage::PairConfirm {
            device: device(),
            mac: Blob(vec![4; MAC_LEN]),
        },
        ControlMessage::PairDone,
        ControlMessage::SessionInit {
            proto: 1,
            device: device(),
            host: device(),
            noise: Blob(vec![5; 300]),
        },
        ControlMessage::SessionAccept {
            noise: Blob(vec![6; 200]),
        },
        ControlMessage::Ping { n: 7 },
        ControlMessage::Pong { n: 7 },
        ControlMessage::Reject {
            reason: RejectReason::BadPin,
        },
        ControlMessage::Reject {
            reason: RejectReason::PairingClosed,
        },
    ]
}

#[test]
fn pair_messages_round_trip() {
    for msg in pair_messages() {
        let bytes = encode(&msg).unwrap();
        assert_eq!(decode(&bytes).unwrap(), msg);
    }
}

#[test]
fn bad_mac_len_rejected() {
    let msg = ControlMessage::PairConfirm {
        device: device(),
        mac: Blob(vec![4; MAC_LEN - 1]),
    };
    assert!(matches!(
        encode(&msg),
        Err(ControlError::BlobLen { field: "mac", .. })
    ));
}

#[test]
fn bad_spake_len_rejected() {
    let msg = ControlMessage::PairStart {
        device: device(),
        spake: Blob(vec![1; SPAKE_LEN + 1]),
    };
    assert!(matches!(
        encode(&msg),
        Err(ControlError::BlobLen { field: "spake", .. })
    ));
}

#[test]
fn noise_too_long_rejected() {
    let msg = ControlMessage::SessionAccept {
        noise: Blob(vec![1; MAX_NOISE + 1]),
    };
    assert!(matches!(
        encode(&msg),
        Err(ControlError::BlobLen { field: "noise", .. })
    ));
}

#[test]
fn public_key_is_64_hex() {
    let json = serde_json::to_string(&PublicKey([0xab; 32])).unwrap();
    assert_eq!(json, format!("\"{}\"", "ab".repeat(32)));
    let short = format!("\"{}\"", "ab".repeat(31));
    assert!(serde_json::from_str::<PublicKey>(&short).is_err());
}

#[test]
fn blob_over_parse_limit_rejected() {
    let json = format!(
        r#"{{"type":"SessionAccept","noise":"{}"}}"#,
        "00".repeat(crate::blob::MAX_BLOB + 1)
    );
    assert!(matches!(
        decode(json.as_bytes()),
        Err(ControlError::Parse(_))
    ));
}

#[test]
fn hello_without_codecs_defaults_to_h264() {
    let json = format!(
        r#"{{"type":"Hello","proto":1,"device":"{DEVICE}","name":"PC",
        "displays":[{{"width":1920,"height":1080,"refresh_mhz":60000,"hdr":false,"dpi":96}}],
        "stream_display":0}}"#
    );
    match decode(json.as_bytes()) {
        Ok(ControlMessage::Hello { codecs, .. }) => assert_eq!(codecs, vec![CodecId::H264]),
        other => panic!("not a hello: {other:?}"),
    }
}

#[test]
fn welcome_without_codec_is_h264() {
    let json = r#"{"type":"Welcome","session":7,
        "mode":{"width":1920,"height":1080,"refresh_mhz":60000,"hdr":false,"dpi":96}}"#;
    match decode(json.as_bytes()) {
        Ok(ControlMessage::Welcome { codec, .. }) => assert_eq!(codec, CodecId::H264),
        other => panic!("not a welcome: {other:?}"),
    }
}

#[test]
fn codec_ids_are_lowercase_in_json() {
    assert_eq!(serde_json::to_string(&CodecId::Hevc).unwrap(), r#""hevc""#);
    assert_eq!(serde_json::to_string(&CodecId::H264).unwrap(), r#""h264""#);
}

#[test]
fn bye_host_error_round_trip() {
    let msg = ControlMessage::Bye {
        reason: ByeReason::HostError,
    };
    assert_eq!(round_trip(&msg), msg);
}
