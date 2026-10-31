use winit::keyboard::KeyCode;

use mixlink_input::keymap;

use super::*;

#[test]
fn plain_and_extended_scancodes() {
    let a = raw_key(KeyCode::KeyA, 0x001E, true);
    assert_eq!((a.scan, a.extended, a.down), (0x1E, false, true));
    let right_ctrl = raw_key(KeyCode::ControlRight, 0xE01D, false);
    assert_eq!(
        (right_ctrl.scan, right_ctrl.extended, right_ctrl.down),
        (0x1D, true, false)
    );
}

#[test]
fn pause_and_numlock_carry_their_vk() {
    let pause = raw_key(KeyCode::Pause, 0x0045, true);
    assert_eq!(
        keymap::usage(pause.scan, pause.extended, pause.vk),
        Some(0x48)
    );
    let num = raw_key(KeyCode::NumLock, 0xE045, true);
    assert_eq!(keymap::usage(num.scan, num.extended, num.vk), Some(0x53));
}

#[test]
fn same_usage_as_the_hook() {
    let up = raw_key(KeyCode::ArrowUp, 0xE048, true);
    assert_eq!(keymap::usage(up.scan, up.extended, up.vk), Some(0x52));
}
