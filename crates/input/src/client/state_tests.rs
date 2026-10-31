use mixlink_core::input::{BUTTON_CODE, InputEvent, MouseMode};

use super::*;

fn key_held(p: &InputPacket, usage: u8) -> bool {
    p.keys[usize::from(usage / 8)] & (1 << (usage % 8)) != 0
}

#[test]
fn repeat_down_adds_no_event() {
    let mut s = InputState::new();
    assert!(s.key(0x04, true));
    assert!(!s.key(0x04, true));
    assert!(!s.key(0x04, true));
    let p = s.packet();
    assert_eq!(p.count, 1);
    assert_eq!(p.first_event, 1);
}

#[test]
fn events_keep_last_eight() {
    let mut s = InputState::new();
    for usage in 0x04..0x0E {
        s.key(usage, true);
    }
    let p = s.packet();
    assert_eq!(p.count, 8);
    assert_eq!(p.first_event, 3);
    assert_eq!(
        p.events[0],
        InputEvent {
            code: 0x06,
            down: true
        }
    );
    assert_eq!(p.events[7].code, 0x0D);
}

#[test]
fn bitmap_matches_held_keys() {
    let mut s = InputState::new();
    s.key(0x04, true);
    s.key(0xE1, true);
    s.key(0x04, false);
    let p = s.packet();
    assert!(!key_held(&p, 0x04));
    assert!(key_held(&p, 0xE1));
}

#[test]
fn button_events_use_button_code() {
    let mut s = InputState::new();
    assert!(s.button(1, true));
    assert!(!s.button(5, true));
    let p = s.packet();
    assert_eq!(p.buttons, 0b10);
    assert_eq!(
        p.events[0],
        InputEvent {
            code: BUTTON_CODE + 1,
            down: true
        }
    );
}

#[test]
fn rel_totals_accumulate() {
    let mut s = InputState::new();
    s.set_mode(MouseMode::Relative);
    s.mouse_rel(3, 1);
    s.mouse_rel(-5, 1);
    s.wheel(120, -60);
    let p = s.packet();
    assert_eq!((p.rel_x, p.rel_y), (-2, 2));
    assert_eq!((p.wheel, p.hwheel), (120, -60));
    assert_eq!(p.mode, MouseMode::Relative);
}

#[test]
fn rel_totals_wrap() {
    let mut s = InputState::new();
    s.mouse_rel(i32::MAX, 0);
    s.mouse_rel(2, 0);
    assert_eq!(s.packet().rel_x, i32::MIN + 1);
}

#[test]
fn abs_position_is_kept() {
    let mut s = InputState::new();
    s.mouse_abs(100, 200);
    let p = s.packet();
    assert_eq!((p.abs_x, p.abs_y), (100, 200));
}

#[test]
fn release_all_emits_up_events() {
    let mut s = InputState::new();
    s.key(0x04, true);
    s.button(0, true);
    s.packet();
    assert!(s.held());
    assert!(s.release_all());
    assert!(!s.held());
    let p = s.packet();
    assert_eq!(p.buttons, 0);
    assert_eq!(p.keys, [0; 32]);
    let ups = p.events[..usize::from(p.count)]
        .iter()
        .filter(|e| !e.down)
        .count();
    assert_eq!(ups, 2);
    assert!(!s.release_all());
}

#[test]
fn repeat_packet_keeps_seq() {
    let mut s = InputState::new();
    s.key(0x04, true);
    let p = s.packet();
    assert_eq!(p.seq, 1);
    assert_eq!(s.current(), p);
    assert_eq!(s.packet().seq, 2);
}
