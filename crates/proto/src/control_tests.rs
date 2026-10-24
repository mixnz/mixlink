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
    };
    assert!(matches!(
        encode(&msg),
        Err(ControlError::Refresh { mhz: 0 })
    ));
}
