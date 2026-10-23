//! Gamepad controls by name, for mapping wizards (Steam "Begin Setup") driven by hand.

use crate::report::PadState;

/// Every control `control` knows, in the order a mapping wizard usually asks for them.
pub const NAMES: [&str; 24] = [
    "a", "b", "x", "y", "lb", "rb", "back", "start", "ls", "rs", "up", "down", "left", "right",
    "lt", "rt", "ls-up", "ls-down", "ls-left", "ls-right", "rs-up", "rs-down", "rs-left",
    "rs-right",
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

#[cfg(test)]
#[path = "padmap_tests.rs"]
mod tests;
