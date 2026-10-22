//! Short scripted input for the game checks (spec 0006 cases H-K). Every scenario ends
//! with all keys released and the pad centered.

use crate::report::PadState;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scenario {
    Walk,
    Look,
    Menu,
    PadWalk,
    PadLook,
    PadMenu,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Key { usage: u8, down: bool },
    MouseMove { dx: i16, dy: i16 },
    Pad(PadState),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Step {
    pub at_ms: u32,
    pub action: Action,
}

const USAGE_W: u8 = 0x1A;
const USAGE_ESC: u8 = 0x29;
/// Gamepad buttons in Xbox order: A=1, B=2, ..., Back=7, Start=8 (bit = button - 1).
const BUTTON_B: u16 = 1 << 1;
const BUTTON_START: u16 = 1 << 7;

impl Scenario {
    pub fn parse(name: &str) -> Option<Scenario> {
        let all = [
            Scenario::Walk,
            Scenario::Look,
            Scenario::Menu,
            Scenario::PadWalk,
            Scenario::PadLook,
            Scenario::PadMenu,
        ];
        all.into_iter()
            .find(|s| format!("{s:?}").eq_ignore_ascii_case(name))
    }
}

fn key(at_ms: u32, usage: u8, down: bool) -> Step {
    Step {
        at_ms,
        action: Action::Key { usage, down },
    }
}

fn pad(at_ms: u32, state: PadState) -> Step {
    Step {
        at_ms,
        action: Action::Pad(state),
    }
}

fn tap(at_ms: u32, usage: u8) -> [Step; 2] {
    [key(at_ms, usage, true), key(at_ms + 50, usage, false)]
}

fn press(at_ms: u32, buttons: u16) -> [Step; 2] {
    [
        pad(
            at_ms,
            PadState {
                buttons,
                ..PadState::CENTERED
            },
        ),
        pad(at_ms + 100, PadState::CENTERED),
    ]
}

pub fn steps(scenario: Scenario) -> Vec<Step> {
    match scenario {
        // Hold W for 2 s.
        Scenario::Walk => vec![key(0, USAGE_W, true), key(2000, USAGE_W, false)],
        // Turn right then back: 50 x +20 px, 50 x -20 px, 10 ms apart.
        Scenario::Look => (0..100u32)
            .map(|n| Step {
                at_ms: n * 10,
                action: Action::MouseMove {
                    dx: if n < 50 { 20 } else { -20 },
                    dy: 0,
                },
            })
            .collect(),
        // Open the pause menu, then close it.
        Scenario::Menu => [tap(0, USAGE_ESC), tap(1500, USAGE_ESC)].concat(),
        // Left stick fully up for 2 s.
        Scenario::PadWalk => vec![
            pad(
                0,
                PadState {
                    ly: 0,
                    ..PadState::CENTERED
                },
            ),
            pad(2000, PadState::CENTERED),
        ],
        // Right stick right for 1 s, left for 1 s.
        Scenario::PadLook => vec![
            pad(
                0,
                PadState {
                    rx: 0xFFFF,
                    ..PadState::CENTERED
                },
            ),
            pad(
                1000,
                PadState {
                    rx: 0,
                    ..PadState::CENTERED
                },
            ),
            pad(2000, PadState::CENTERED),
        ],
        // Start opens the menu, B closes it.
        Scenario::PadMenu => [press(0, BUTTON_START), press(1500, BUTTON_B)].concat(),
    }
}

#[cfg(test)]
#[path = "scenario_tests.rs"]
mod tests;
