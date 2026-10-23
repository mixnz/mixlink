use super::*;
use crate::report::PadState;

fn pressed(name: &str) -> PadState {
    control(name).unwrap_or_else(|| panic!("{name} not mapped"))
}

#[test]
fn face_and_shoulder_buttons_set_one_bit() {
    let table = [
        ("a", 0),
        ("b", 1),
        ("x", 2),
        ("y", 3),
        ("lb", 4),
        ("rb", 5),
        ("back", 6),
        ("start", 7),
        ("ls", 8),
        ("rs", 9),
    ];
    for (name, bit) in table {
        assert_eq!(
            pressed(name),
            PadState {
                buttons: 1 << bit,
                ..PadState::CENTERED
            },
            "{name}"
        );
    }
}

#[test]
fn dpad_uses_the_hat() {
    for (name, hat) in [("up", 0), ("right", 2), ("down", 4), ("left", 6)] {
        assert_eq!(
            pressed(name),
            PadState {
                hat,
                ..PadState::CENTERED
            },
            "{name}"
        );
    }
}

#[test]
fn triggers_go_full() {
    assert_eq!(
        pressed("lt"),
        PadState {
            lt: 255,
            ..PadState::CENTERED
        }
    );
    assert_eq!(
        pressed("rt"),
        PadState {
            rt: 255,
            ..PadState::CENTERED
        }
    );
}

#[test]
fn sticks_go_to_the_edge() {
    assert_eq!(
        pressed("ls-up"),
        PadState {
            ly: 0,
            ..PadState::CENTERED
        }
    );
    assert_eq!(
        pressed("ls-down"),
        PadState {
            ly: 0xFFFF,
            ..PadState::CENTERED
        }
    );
    assert_eq!(
        pressed("ls-left"),
        PadState {
            lx: 0,
            ..PadState::CENTERED
        }
    );
    assert_eq!(
        pressed("ls-right"),
        PadState {
            lx: 0xFFFF,
            ..PadState::CENTERED
        }
    );
    assert_eq!(
        pressed("rs-up"),
        PadState {
            ry: 0,
            ..PadState::CENTERED
        }
    );
    assert_eq!(
        pressed("rs-down"),
        PadState {
            ry: 0xFFFF,
            ..PadState::CENTERED
        }
    );
    assert_eq!(
        pressed("rs-left"),
        PadState {
            rx: 0,
            ..PadState::CENTERED
        }
    );
    assert_eq!(
        pressed("rs-right"),
        PadState {
            rx: 0xFFFF,
            ..PadState::CENTERED
        }
    );
}

#[test]
fn names_ignore_case_and_spaces() {
    assert_eq!(control("  START "), control("start"));
    assert_eq!(control("Ls-Up"), control("ls-up"));
}

#[test]
fn unknown_names_are_rejected() {
    assert_eq!(control("guide"), None, "no guide button in the descriptor");
    assert_eq!(control(""), None);
    assert_eq!(control("ls-diagonal"), None);
}

#[test]
fn every_listed_name_is_mapped() {
    for name in NAMES {
        assert!(control(name).is_some(), "{name}");
    }
    assert_eq!(NAMES.len(), 24);
}
