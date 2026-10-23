use super::*;
use crate::report::PadState;

#[test]
fn keyboard_state_press_release() {
    let mut kb = KeyboardState::default();
    assert_eq!(kb.apply(0x68, true), [0, 0, 0x68, 0, 0, 0, 0, 0]);
    assert_eq!(kb.apply(0x1A, true), [0, 0, 0x68, 0x1A, 0, 0, 0, 0]);
    assert_eq!(kb.apply(0x68, false), [0, 0, 0x1A, 0, 0, 0, 0, 0]);
    assert_eq!(kb.apply(0x1A, false), [0; 8]);
}

#[test]
fn keyboard_state_ignores_repeat_and_seventh_key() {
    let mut kb = KeyboardState::default();
    kb.apply(4, true);
    assert_eq!(kb.apply(4, true), [0, 0, 4, 0, 0, 0, 0, 0], "repeat press");
    for k in 5..=9 {
        kb.apply(k, true);
    }
    assert_eq!(
        kb.apply(10, true),
        [0, 0, 4, 5, 6, 7, 8, 9],
        "seventh key dropped"
    );
    assert_eq!(
        kb.apply(99, false),
        [0, 0, 4, 5, 6, 7, 8, 9],
        "release of unheld key"
    );
}

#[test]
fn usage_to_vk_known_keys() {
    assert_eq!(usage_to_vk(0x68), Some(0x7C), "F13");
    assert_eq!(usage_to_vk(0x73), Some(0x87), "F24");
    assert_eq!(usage_to_vk(0x1A), Some(0x57), "W");
    assert_eq!(usage_to_vk(0x29), Some(0x1B), "Esc");
    assert_eq!(usage_to_vk(0x2B), Some(0x09), "Tab");
    assert_eq!(usage_to_vk(0x2C), Some(0x20), "Space");
    assert_eq!(usage_to_vk(0x28), None, "Enter is never sent");
}

#[test]
fn pad_to_xinput_centered_is_zero() {
    let x = pad_to_xinput(&PadState::CENTERED);
    assert_eq!(x, XinputPad::default());
}

#[test]
fn pad_to_xinput_axes_and_y_flip() {
    let x = pad_to_xinput(&PadState {
        lx: 0xFFFF,
        ly: 0,
        rx: 0,
        ry: 0xFFFF,
        lt: 10,
        rt: 255,
        ..PadState::CENTERED
    });
    assert_eq!(x.thumb_lx, 32767);
    assert_eq!(x.thumb_ly, 32767, "HID up (0) is XInput up (+)");
    assert_eq!(x.thumb_rx, -32768);
    assert_eq!(
        x.thumb_ry, -32767,
        "HID down (max) is XInput down; center stays 0"
    );
    assert_eq!((x.left_trigger, x.right_trigger), (10, 255));
}

#[test]
fn pad_to_xinput_lx_matches_sequence_code() {
    for i in [0u32, 1, 0x7FFF, 0x8000, 0xFFFF] {
        let state = PadState {
            lx: crate::seq::encode_pad_lx(i),
            ..PadState::CENTERED
        };
        assert_eq!(
            pad_to_xinput(&state).thumb_lx,
            crate::seq::encode_xinput_lx(i)
        );
    }
}

#[test]
fn pad_to_xinput_buttons_and_hat() {
    let x = pad_to_xinput(&PadState {
        buttons: 0b11_1111_1111,
        hat: 1,
        ..PadState::CENTERED
    });
    // A B X Y LB RB Back Start LS RS + D-pad up-right.
    let expected = 0x1000
        | 0x2000
        | 0x4000
        | 0x8000
        | 0x0100
        | 0x0200
        | 0x0020
        | 0x0010
        | 0x0040
        | 0x0080
        | 0x0001
        | 0x0008;
    assert_eq!(x.buttons, expected);
    assert_eq!(
        pad_to_xinput(&PadState {
            hat: 4,
            ..PadState::CENTERED
        })
        .buttons,
        0x0002
    );
    assert_eq!(
        pad_to_xinput(&PadState {
            hat: 6,
            ..PadState::CENTERED
        })
        .buttons,
        0x0004
    );
}

#[test]
fn pad_to_xinput_guide_and_share() {
    let guide = PadState {
        buttons: 1 << 10,
        ..PadState::CENTERED
    };
    assert_eq!(pad_to_xinput(&guide).buttons, 0x0400, "XInput guide");
    let share = PadState {
        buttons: 1 << 11,
        ..PadState::CENTERED
    };
    assert_eq!(
        pad_to_xinput(&share).buttons,
        0,
        "XInput has no share button"
    );
}
