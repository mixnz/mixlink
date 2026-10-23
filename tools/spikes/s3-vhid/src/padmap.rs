//! Gamepad controls by name, for mapping wizards (Steam "Begin Setup") driven by hand.

use crate::report::PadState;

/// Every control `control` knows, in the order a mapping wizard usually asks for them.
pub const NAMES: [&str; 30] = [
    "a", "b", "x", "y", "lb", "rb", "back", "start", "ls", "rs", "guide", "share", "up", "down",
    "left", "right", "lt", "rt", "ls-up", "ls-down", "ls-left", "ls-right", "rs-up", "rs-down",
    "rs-left", "rs-right", "b13", "b14", "b15", "b16",
];

/// Pad state with only `name` pressed (buttons, D-pad, full trigger or a stick at its edge).
pub fn control(name: &str) -> Option<PadState> {
    let c = PadState::CENTERED;
    let button = |bit: u16| PadState {
        buttons: 1 << bit,
        ..c
    };
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
        // Spare buttons: Steam can bind them to anything (e.g. LT / RT as digital triggers).
        "b13" => button(12),
        "b14" => button(13),
        "b15" => button(14),
        "b16" => button(15),
        "up" => PadState { hat: 0, ..c },
        "right" => PadState { hat: 2, ..c },
        "down" => PadState { hat: 4, ..c },
        "left" => PadState { hat: 6, ..c },
        "lt" => PadState { lt: 255, ..c },
        "rt" => PadState { rt: 255, ..c },
        "ls-up" => PadState { ly: 0, ..c },
        "ls-down" => PadState { ly: 0xFFFF, ..c },
        "ls-left" => PadState { lx: 0, ..c },
        "ls-right" => PadState { lx: 0xFFFF, ..c },
        "rs-up" => PadState { ry: 0, ..c },
        "rs-down" => PadState { ry: 0xFFFF, ..c },
        "rs-left" => PadState { rx: 0, ..c },
        "rs-right" => PadState { rx: 0xFFFF, ..c },
        _ => return None,
    })
}

pub const DEFAULT_HOLD_MS: u32 = 500;
const MAX_HOLD_MS: u32 = 10_000;

/// One line typed in `s3-feed --pad-interactive`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    Quit,
    Skip,
    /// Nudge every analog off rest and back (see `prime_steps`).
    Prime,
    Press {
        state: PadState,
        hold_ms: u32,
    },
    Unknown(String),
}

/// `<control> [hold_ms]`, `prime`, `skip` / empty, `q` / `quit`.
pub fn parse_command(line: &str) -> Command {
    let lower = line.trim().to_ascii_lowercase();
    let words: Vec<&str> = lower.split_whitespace().collect();
    match words.as_slice() {
        [] | ["skip"] => Command::Skip,
        ["q"] | ["quit"] => Command::Quit,
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

/// Moves each trigger and stick axis a little off rest and back, one at a time, so a
/// reader that learns an axis' rest value from its first change (SDL / Steam) sees rest.
pub fn prime_steps() -> Vec<PadState> {
    let c = PadState::CENTERED;
    let nudges = [
        PadState { lt: 16, ..c },
        PadState { rt: 16, ..c },
        PadState { lx: 0x9000, ..c },
        PadState { ly: 0x9000, ..c },
        PadState { rx: 0x9000, ..c },
        PadState { ry: 0x9000, ..c },
    ];
    nudges.iter().flat_map(|n| [*n, c]).collect()
}

#[cfg(test)]
#[path = "padmap_tests.rs"]
mod tests;
