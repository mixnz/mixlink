use super::*;

#[test]
fn control_report_layout() {
    let payload = pad_payload(&PadState::CENTERED);
    let report = control_report(Device::Gamepad, &payload).unwrap();
    assert_eq!(report.len(), CONTROL_REPORT_LEN);
    assert_eq!(report[0], CONTROL_REPORT_ID);
    assert_eq!(report[1], 4);
    assert_eq!(&report[2..], &payload[..]);

    let kbd = kbd_payload(0, &[0x68]);
    let report = control_report(Device::Keyboard, &kbd).unwrap();
    assert_eq!(&report[..10], &[5, 1, 0, 0, 0x68, 0, 0, 0, 0, 0]);
    assert_eq!(&report[10..], &[0; 5], "zero-padded to 13 payload bytes");
}

#[test]
fn control_report_rejects_long_payload() {
    assert_eq!(
        control_report(Device::MouseAbs, &[0; 6]),
        Err(ReportError::WrongLength {
            device: Device::MouseAbs,
            len: 6
        })
    );
    assert!(control_report(Device::Gamepad, &[0; 14]).is_err());
}

#[test]
fn report_ids_and_sizes_match_driver() {
    let table = [
        (Device::Keyboard, 1, 8),
        (Device::MouseRel, 2, 7),
        (Device::MouseAbs, 3, 5),
        (Device::Gamepad, 4, 13),
    ];
    for (device, id, len) in table {
        assert_eq!(report_id(device), id);
        assert_eq!(payload_len(device), len);
    }
}

#[test]
fn pad_payload_little_endian() {
    let state = PadState {
        buttons: 0x8001,
        hat: 2,
        lx: 0x1234,
        ly: 0xABCD,
        rx: 0x0001,
        ry: 0xFFFF,
        lt: 7,
        rt: 255,
    };
    assert_eq!(
        pad_payload(&state),
        [
            0x01, 0x80, 2, 0x34, 0x12, 0xCD, 0xAB, 0x01, 0x00, 0xFF, 0xFF, 7, 255
        ]
    );
}

#[test]
fn centered_pad_has_null_hat_and_mid_sticks() {
    let p = pad_payload(&PadState::CENTERED);
    assert_eq!(p[2], 8);
    assert_eq!(&p[3..11], &[0x00, 0x80, 0x00, 0x80, 0x00, 0x80, 0x00, 0x80]);
}

#[test]
fn kbd_f13_usage_is_0x68() {
    assert_eq!(USAGE_F13, 0x68);
    assert_eq!(
        kbd_payload(0x02, &[USAGE_F13]),
        [0x02, 0, 0x68, 0, 0, 0, 0, 0]
    );
}

#[test]
fn kbd_payload_keeps_six_keys() {
    let p = kbd_payload(0, &[4, 5, 6, 7, 8, 9, 10]);
    assert_eq!(&p[2..], &[4, 5, 6, 7, 8, 9]);
}

#[test]
fn mouse_payloads() {
    assert_eq!(
        mouse_rel_payload(1, -2, 300, -1),
        [1, 0xFE, 0xFF, 0x2C, 0x01, 0xFF, 0]
    );
    assert_eq!(mouse_abs_payload(0, 32767, 1), [0, 0xFF, 0x7F, 1, 0]);
}
