//! Scripted input for `input try` (spec 0016). Side-effect free: F13..F24 only, mouse moves that
//! come back home, pad sweeps that end centred.

use std::time::Duration;

use mixlink_input::hid::gamepad::{Dpad, PadState};
use mixlink_input::hid::keyboard::KeyboardState;
use mixlink_input::hid::mouse::{mouse_abs_report, mouse_rel_reports};
use mixlink_input::platform::windows::vhid::{ReportSink, VHidError};

pub use crate::args::Script;

const F13: u8 = 0x68;
const F24: u8 = 0x73;
const F22: u8 = 0x71;
const ABS_MAX: u16 = 32_767;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Key(u8, bool),
    Rel(i32, i32),
    Abs(u16, u16),
    Pad(PadState),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Step {
    /// Time from the start of the script.
    pub at_ms: u32,
    pub action: Action,
}

pub fn steps(script: Script) -> Vec<Step> {
    match script {
        Script::Keys => keys(0),
        Script::Chord => chord(),
        Script::Mouse => mouse(0),
        Script::Abs => abs(0),
        Script::Pad => pad(0),
        Script::All => {
            let mut all = keys(0);
            for part in [mouse, abs, pad] {
                let start = all.last().map_or(0, |s| s.at_ms) + 500;
                all.extend(part(start));
            }
            all
        }
        Script::Hold => vec![step(0, Action::Key(F13, true))],
    }
}

/// Feeds `steps` in time order. `wait(d)` sleeps up to `d` and returns true to stop early; stopping
/// leaves held keys to the driver's release-all when the handle closes.
pub fn play(
    steps: &[Step],
    sink: &mut impl ReportSink,
    wait: &mut dyn FnMut(Duration) -> bool,
) -> Result<(), VHidError> {
    let mut keyboard = KeyboardState::default();
    let mut now_ms = 0u32;
    for step in steps {
        if step.at_ms > now_ms {
            if wait(Duration::from_millis(u64::from(step.at_ms - now_ms))) {
                return Ok(());
            }
            now_ms = step.at_ms;
        }
        match step.action {
            Action::Key(usage, down) => {
                keyboard.set(usage, down);
                sink.feed(&keyboard.report())?;
            }
            Action::Rel(dx, dy) => {
                for report in mouse_rel_reports(0, dx, dy, 0, 0).as_slice() {
                    sink.feed(report)?;
                }
            }
            Action::Abs(x, y) => sink.feed(&mouse_abs_report(0, x, y))?,
            Action::Pad(state) => sink.feed(&state.report())?,
        }
    }
    Ok(())
}

fn step(at_ms: u32, action: Action) -> Step {
    Step { at_ms, action }
}

/// Each of F13..F24 pressed for 50 ms, 100 ms apart.
fn keys(start: u32) -> Vec<Step> {
    let mut out = Vec::new();
    for (i, usage) in (F13..=F24).enumerate() {
        let at = start + 100 * u32::try_from(i).unwrap_or(0);
        out.push(step(at, Action::Key(usage, true)));
        out.push(step(at + 50, Action::Key(usage, false)));
    }
    out
}

fn chord() -> Vec<Step> {
    let mut out: Vec<Step> = (F13..=F22)
        .map(|usage| step(0, Action::Key(usage, true)))
        .collect();
    out.extend((F13..=F22).map(|usage| step(1000, Action::Key(usage, false))));
    out
}

/// A 200 px square, 10 px every 10 ms.
fn mouse(start: u32) -> Vec<Step> {
    let mut out = Vec::new();
    let mut at = start;
    for (dx, dy) in [(10, 0), (0, 10), (-10, 0), (0, -10)] {
        for _ in 0..20 {
            out.push(step(at, Action::Rel(dx, dy)));
            at += 10;
        }
    }
    out
}

/// Four corners of the desktop, then its centre, 500 ms apart.
fn abs(start: u32) -> Vec<Step> {
    let points = [
        (0, 0),
        (ABS_MAX, 0),
        (ABS_MAX, ABS_MAX),
        (0, ABS_MAX),
        (ABS_MAX / 2, ABS_MAX / 2),
    ];
    (0u32..)
        .zip(points)
        .map(|(i, (x, y))| step(start + 500 * i, Action::Abs(x, y)))
        .collect()
}

/// Each stick to both ends, both triggers, the d-pad, buttons 1..16 one by one, then centred.
fn pad(start: u32) -> Vec<Step> {
    let n = PadState::NEUTRAL;
    let mut states = vec![
        PadState { lx: i16::MIN, ..n },
        PadState { lx: i16::MAX, ..n },
        PadState { ly: i16::MIN, ..n },
        PadState { ly: i16::MAX, ..n },
        PadState { rx: i16::MIN, ..n },
        PadState { rx: i16::MAX, ..n },
        PadState { ry: i16::MIN, ..n },
        PadState { ry: i16::MAX, ..n },
        PadState { lt: u16::MAX, ..n },
        PadState { rt: u16::MAX, ..n },
    ];
    for dpad in [Dpad::Up, Dpad::Right, Dpad::Down, Dpad::Left] {
        states.push(PadState { dpad, ..n });
    }
    for bit in 0..16 {
        states.push(PadState {
            buttons: 1 << bit,
            ..n
        });
    }
    states.push(n);
    (0u32..)
        .zip(states)
        .map(|(i, state)| step(start + 200 * i, Action::Pad(state)))
        .collect()
}

#[cfg(test)]
#[path = "script_tests.rs"]
mod tests;
