//! Pad controls by name for `input pad` (spec 0016): drive a mapping wizard (Steam "Begin Setup")
//! by typing `a`, `lt`, ... Ported from the spec 0006 harness.

use mixlink_input::hid::gamepad::{Dpad, PadState};

/// Every control, in the order a mapping wizard usually asks for them.
pub const NAMES: [&str; 30] = [
    "a", "b", "x", "y", "lb", "rb", "back", "start", "ls", "rs", "guide", "share", "up", "down",
    "left", "right", "lt", "rt", "ls-up", "ls-down", "ls-left", "ls-right", "rs-up", "rs-down",
    "rs-left", "rs-right", "b13", "b14", "b15", "b16",
];

pub const DEFAULT_HOLD_MS: u32 = 500;
const MAX_HOLD_MS: u32 = 10_000;

/// Pad state with only `name` pressed: a button, a d-pad direction, a full trigger or a stick at
/// its edge (+y = down).
pub fn control(name: &str) -> Option<PadState> {
    let n = PadState::NEUTRAL;
    let button = |bit: u16| PadState {
        buttons: 1 << bit,
        ..n
    };
    let dpad = |dpad| PadState { dpad, ..n };
    Some(match name.trim().to_ascii_lowercase().as_str() {
        "a" => button(0),
        "b" => button(1),
        "x" => button(2),
        "y" => button(3),
        "lb" => button(4),
        "rb" => button(5),
        "back" => button(6),
        "start" => button(7),
        "ls" => button(8),
        "rs" => button(9),
        "guide" => button(10),
        "share" => button(11),
        "b13" => button(12),
        "b14" => button(13),
        "b15" => button(14),
        "b16" => button(15),
        "up" => dpad(Dpad::Up),
        "right" => dpad(Dpad::Right),
        "down" => dpad(Dpad::Down),
        "left" => dpad(Dpad::Left),
        "lt" => PadState { lt: u16::MAX, ..n },
        "rt" => PadState { rt: u16::MAX, ..n },
        "ls-up" => PadState { ly: i16::MIN, ..n },
        "ls-down" => PadState { ly: i16::MAX, ..n },
        "ls-left" => PadState { lx: i16::MIN, ..n },
        "ls-right" => PadState { lx: i16::MAX, ..n },
        "rs-up" => PadState { ry: i16::MIN, ..n },
        "rs-down" => PadState { ry: i16::MAX, ..n },
        "rs-left" => PadState { rx: i16::MIN, ..n },
        "rs-right" => PadState { rx: i16::MAX, ..n },
        _ => return None,
    })
}

/// Reports per ramp: a real trigger or stick passes through many values, and a mapping wizard that
/// takes its rest value from the first motion event never binds a single jump (spec 0016).
pub const RAMP_STEPS: usize = 10;

/// States from rest to `target`, moving every axis an equal share per step. Buttons and d-pad
/// have no in-between value, so a target without axes is one step.
pub fn ramp(target: PadState, steps: usize) -> Vec<PadState> {
    let n = PadState::NEUTRAL;
    let analog = target.lx != 0
        || target.ly != 0
        || target.rx != 0
        || target.ry != 0
        || target.lt != 0
        || target.rt != 0;
    if !analog || steps < 2 {
        return vec![target];
    }
    let total = i64::try_from(steps).unwrap_or(1);
    let part_i16 = |v: i16, k: i64| i16::try_from(i64::from(v) * k / total).unwrap_or(v);
    let part_u16 = |v: u16, k: i64| u16::try_from(i64::from(v) * k / total).unwrap_or(v);
    (1..=total)
        .map(|k| PadState {
            lx: part_i16(target.lx, k),
            ly: part_i16(target.ly, k),
            rx: part_i16(target.rx, k),
            ry: part_i16(target.ry, k),
            lt: part_u16(target.lt, k),
            rt: part_u16(target.rt, k),
            ..if k == total { target } else { n }
        })
        .collect()
}

/// One line typed in `input pad`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Quit,
    Skip,
    /// Nudge every trigger and stick off rest and back (`prime_steps`).
    Prime,
    Press {
        state: PadState,
        hold_ms: u32,
    },
    Unknown(String),
}

/// `<control> [hold_ms]`, `prime`, `skip` / empty line, `q` / `quit`.
pub fn parse_command(line: &str) -> Command {
    let lower = line.trim().to_ascii_lowercase();
    let words: Vec<&str> = lower.split_whitespace().collect();
    match words.as_slice() {
        [] | ["skip"] => Command::Skip,
        ["q" | "quit"] => Command::Quit,
        ["prime"] => Command::Prime,
        [name] => match control(name) {
            Some(state) => Command::Press {
                state,
                hold_ms: DEFAULT_HOLD_MS,
            },
            None => Command::Unknown(lower),
        },
        [name, hold] => match (control(name), hold.parse::<u32>()) {
            (Some(state), Ok(ms)) if (1..=MAX_HOLD_MS).contains(&ms) => {
                Command::Press { state, hold_ms: ms }
            }
            _ => Command::Unknown(lower),
        },
        _ => Command::Unknown(lower),
    }
}

/// Each trigger and stick axis a little off rest and back, one at a time, so a reader that learns
/// an axis' rest value from its first change (SDL / Steam) sees rest.
pub fn prime_steps() -> Vec<PadState> {
    let n = PadState::NEUTRAL;
    let nudges = [
        PadState { lt: 4096, ..n },
        PadState { rt: 4096, ..n },
        PadState { lx: 4096, ..n },
        PadState { ly: 4096, ..n },
        PadState { rx: 4096, ..n },
        PadState { ry: 4096, ..n },
    ];
    nudges.iter().flat_map(|nudge| [*nudge, n]).collect()
}

#[cfg(test)]
#[path = "padmap_tests.rs"]
mod tests;
