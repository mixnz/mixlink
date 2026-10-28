use super::*;

fn sample() -> InputPacket {
    let mut keys = [0u8; 32];
    keys[0x04 / 8] |= 1 << (0x04 % 8);
    keys[0xE1 / 8] |= 1 << (0xE1 % 8);
    let mut events = [InputEvent::default(); MAX_EVENTS];
    events[0] = InputEvent {
        code: 0x04,
        down: true,
    };
    events[1] = InputEvent {
        code: 0xE1,
        down: true,
    };
    events[2] = InputEvent {
        code: BUTTON_CODE + 1,
        down: false,
    };
    InputPacket {
        seq: 0x0102_0304,
        mode: MouseMode::Relative,
        buttons: 0b10,
        abs_x: 1919,
        abs_y: 1079,
        rel_x: 1_000_000,
        rel_y: 7,
        wheel: 240,
        hwheel: 120,
        first_event: 41,
        count: 3,
        events,
        keys,
    }
}

fn round_trip(packet: &InputPacket) -> InputPacket {
    let mut out = [0u8; INPUT_LEN];
    packet.encode(&mut out);
    InputPacket::decode(&out).unwrap()
}

#[test]
fn packet_round_trip() {
    let packet = sample();
    assert_eq!(round_trip(&packet), packet);
}

#[test]
fn negative_totals_round_trip() {
    let packet = InputPacket {
        rel_x: -5,
        wheel: i32::MIN,
        ..sample()
    };
    assert_eq!(round_trip(&packet), packet);
}

#[test]
fn short_is_error() {
    let mut out = [0u8; INPUT_LEN];
    sample().encode(&mut out);
    assert_eq!(
        InputPacket::decode(&out[..INPUT_LEN - 1]),
        Err(InputError::TooShort { len: INPUT_LEN - 1 })
    );
}

#[test]
fn unknown_kind_is_error() {
    let mut out = [0u8; INPUT_LEN];
    sample().encode(&mut out);
    out[0] = 2;
    assert_eq!(InputPacket::decode(&out), Err(InputError::Kind { byte: 2 }));
}

#[test]
fn bad_mode_is_error() {
    let mut out = [0u8; INPUT_LEN];
    sample().encode(&mut out);
    out[5] = 3;
    assert_eq!(InputPacket::decode(&out), Err(InputError::Mode { byte: 3 }));
}

#[test]
fn count_over_eight_is_error() {
    let mut out = [0u8; INPUT_LEN];
    sample().encode(&mut out);
    out[31] = 9;
    assert_eq!(
        InputPacket::decode(&out),
        Err(InputError::Count { count: 9 })
    );
}

#[test]
fn unused_event_slots_are_zero_after_decode() {
    let mut packet = sample();
    packet.events[5] = InputEvent {
        code: 0x30,
        down: true,
    };
    let back = round_trip(&packet);
    assert_eq!(back.events[5], InputEvent::default());
}
