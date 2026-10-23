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
    assert_eq!(control("home"), None, "the name is guide");
    assert_eq!(control(""), None);
    assert_eq!(control("ls-diagonal"), None);
}

#[test]
fn every_listed_name_is_mapped() {
    for name in NAMES {
        assert!(control(name).is_some(), "{name}");
    }
    assert_eq!(NAMES.len(), 26);
}

#[test]
fn guide_and_share_use_buttons_11_and_12() {
    assert_eq!(pressed("guide").buttons, 1 << 10);
    assert_eq!(pressed("share").buttons, 1 << 11);
}

#[test]
fn parse_press_with_default_and_custom_hold() {
    assert_eq!(
        parse_command("lt"),
        Command::Press {
            state: pressed("lt"),
            hold_ms: DEFAULT_HOLD_MS
        }
    );
    assert_eq!(
        parse_command("  LT 2000 "),
        Command::Press {
            state: pressed("lt"),
            hold_ms: 2000
        }
    );
}

#[test]
fn parse_rejects_bad_hold() {
    assert!(matches!(parse_command("lt 0"), Command::Unknown(_)));
    assert!(matches!(parse_command("lt 20000"), Command::Unknown(_)));
    assert!(matches!(parse_command("lt soon"), Command::Unknown(_)));
    assert!(matches!(parse_command("lt 500 extra"), Command::Unknown(_)));
}

#[test]
fn parse_keywords() {
    assert_eq!(parse_command("q"), Command::Quit);
    assert_eq!(parse_command("quit"), Command::Quit);
    assert_eq!(parse_command(""), Command::Skip);
    assert_eq!(parse_command("skip"), Command::Skip);
    assert_eq!(parse_command("Prime"), Command::Prime);
    assert!(matches!(parse_command("home"), Command::Unknown(_)));
}

#[test]
fn prime_nudges_each_analog_once_and_ends_centered() {
    let steps = prime_steps();
    assert_eq!(steps.len(), 12, "six analogs x (nudge, rest)");
    assert_eq!(*steps.last().unwrap(), PadState::CENTERED);
    for pair in steps.chunks(2) {
        assert_ne!(pair[0], PadState::CENTERED, "nudge must differ from rest");
        assert_eq!(pair[1], PadState::CENTERED, "each nudge returns to rest");
        assert_eq!(pair[0].buttons, 0);
        assert_eq!(pair[0].hat, 8);
    }
    let lt = &steps[0];
    assert!(lt.lt > 0 && lt.lt < 64, "small trigger nudge, not a press");
}
