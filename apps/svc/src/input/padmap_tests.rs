use super::*;

#[test]
fn every_name_maps_to_a_control() {
    for name in NAMES {
        let state = control(name).unwrap_or_else(|| panic!("{name}"));
        assert_ne!(state, PadState::NEUTRAL, "{name}");
    }
}

#[test]
fn controls_match_the_layout() {
    assert_eq!(control("a").unwrap().buttons, 1);
    assert_eq!(control("B").unwrap().buttons, 2);
    assert_eq!(control("b16").unwrap().buttons, 0x8000);
    assert_eq!(control("up").unwrap().dpad, Dpad::Up);
    assert_eq!(control("lt").unwrap().lt, u16::MAX);
    assert_eq!(control("rt").unwrap().rt, u16::MAX);
    assert_eq!(control("ls-up").unwrap().ly, i16::MIN);
    assert_eq!(control("rs-right").unwrap().rx, i16::MAX);
    assert_eq!(control("dance"), None);
}

#[test]
fn parse_commands() {
    assert_eq!(
        parse_command(" A "),
        Command::Press {
            state: control("a").unwrap(),
            hold_ms: DEFAULT_HOLD_MS
        }
    );
    assert_eq!(
        parse_command("lt 2000"),
        Command::Press {
            state: control("lt").unwrap(),
            hold_ms: 2000
        }
    );
    assert_eq!(parse_command(""), Command::Skip);
    assert_eq!(parse_command("q"), Command::Quit);
    assert_eq!(parse_command("prime"), Command::Prime);
    assert!(matches!(parse_command("lt 0"), Command::Unknown(_)));
    assert!(matches!(parse_command("lt 20000"), Command::Unknown(_)));
    assert!(matches!(parse_command("a b c"), Command::Unknown(_)));
}

#[test]
fn prime_returns_to_rest() {
    let steps = prime_steps();
    assert_eq!(steps.len(), 12);
    assert!(
        steps
            .iter()
            .skip(1)
            .step_by(2)
            .all(|s| *s == PadState::NEUTRAL)
    );
    assert!(steps.iter().step_by(2).all(|s| *s != PadState::NEUTRAL));
}

#[test]
fn ramp_moves_axes_step_by_step() {
    let target = control("lt").unwrap();
    let up = ramp(target, RAMP_STEPS);
    assert_eq!(up.len(), RAMP_STEPS);
    assert!(up.windows(2).all(|w| w[0].lt < w[1].lt), "strictly rising");
    assert_eq!(*up.last().unwrap(), target);
    assert!(up[0].lt > 0 && up[0].lt < u16::MAX / 4);

    let stick = control("ls-left").unwrap();
    let left = ramp(stick, RAMP_STEPS);
    assert!(
        left.windows(2).all(|w| w[0].lx > w[1].lx),
        "strictly falling"
    );
    assert_eq!(*left.last().unwrap(), stick);
}

#[test]
fn ramp_presses_buttons_at_once() {
    let a = control("a").unwrap();
    assert_eq!(ramp(a, RAMP_STEPS), vec![a]);
}
