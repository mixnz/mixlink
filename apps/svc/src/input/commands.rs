//! The `input ...` developer commands (spec 0016).

use std::io::{BufRead, Write};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use mixlink_input::hid::xbox::PadState;
use mixlink_input::platform::windows::vhid::VHid;

use super::open_feeder;
use super::padmap::{self, Command};
use super::pads::{self, AddOutcome};
use super::probe_run;
use super::script::{Script, play, steps};
use super::status::format_status;
use crate::args::InputCmd;
use crate::drivers::flow::RemoveOutcome;
use crate::error::SvcError;
use crate::platform::windows::console::install_ctrl_handler;
use crate::platform::windows::pad_nodes::WindowsPadHost;

use crate::stop::StopSignal;
use crate::strings;

/// `input pad` `prime`: time per nudge.
const PRIME_STEP: Duration = Duration::from_millis(100);
/// `input pad`: time per ramp step (10 steps ≈ 0.2 s, like a quick real pull).
const RAMP_STEP: Duration = Duration::from_millis(20);

pub fn run(cmd: &InputCmd) -> Result<(), SvcError> {
    match cmd {
        InputCmd::Status => status(),
        InputCmd::PadAdd { slot } => pad_add(*slot),
        InputCmd::PadRemove { slot } => pad_remove(*slot),
        InputCmd::Pad { delay_s, slot } => pad_interactive(*delay_s, *slot),
        InputCmd::Try {
            script,
            delay_s,
            slot,
        } => try_script(*script, *delay_s, *slot),
        InputCmd::Probe {
            device,
            count,
            slot,
        } => probe_run::probe(*device, *count, *slot),
    }
}

fn status() -> Result<(), SvcError> {
    let reply = VHid::open()?.query()?;
    std::io::stdout().write_all(format_status(&reply).as_bytes())?;
    Ok(())
}

/// Plays `script` as the driver's feeder. Fails with the "in use" message while the service holds
/// the driver. Ctrl+C stops early; closing the handle releases whatever was held.
fn try_script(script: Script, delay_s: u32, slot: u8) -> Result<(), SvcError> {
    let mut vhid = open_feeder()?;
    let stop = Arc::new(StopSignal::new());
    install_ctrl_handler(Arc::clone(&stop))?;
    for left in (1..=delay_s).rev() {
        eprintln!("{}", strings::input_try_countdown(left));
        if stop.wait_timeout(Duration::from_secs(1)) {
            return Ok(());
        }
    }
    let mut wait = |timeout| stop.wait_timeout(timeout);
    play(&steps(script), slot, &mut vhid, &mut wait)?;
    if script == Script::Hold {
        eprintln!("{}", strings::INPUT_TRY_HOLD);
        stop.wait();
    } else {
        eprintln!("{}", strings::INPUT_TRY_DONE);
    }
    Ok(())
}

/// Reads `<control> [hold_ms]` lines from stdin and presses that control, for mapping wizards
/// (Steam "Begin Setup"). The pad is centred after each press and when the command ends.
fn pad_interactive(delay_s: u32, slot: u8) -> Result<(), SvcError> {
    let vhid = open_feeder()?;
    eprintln!("{}", strings::input_pad_help(&padmap::NAMES.join(" ")));
    for line in std::io::stdin().lock().lines() {
        match padmap::parse_command(&line?) {
            Command::Quit => break,
            Command::Skip => {}
            Command::Prime => {
                for state in padmap::prime_steps() {
                    vhid.feed(&state.report(slot))?;
                    thread::sleep(PRIME_STEP);
                }
                eprintln!("{}", strings::INPUT_PAD_PRIMED);
            }
            Command::Press { state, hold_ms } => {
                if delay_s > 0 {
                    eprintln!("{}", strings::input_try_countdown(delay_s));
                    thread::sleep(Duration::from_secs(u64::from(delay_s)));
                }
                // Axes ramp in and out like a real trigger or stick; buttons are one step.
                let ramp = padmap::ramp(state, padmap::RAMP_STEPS);
                for step in &ramp {
                    vhid.feed(&step.report(slot))?;
                    thread::sleep(RAMP_STEP);
                }
                thread::sleep(Duration::from_millis(u64::from(hold_ms)));
                for step in ramp.iter().rev().skip(1) {
                    vhid.feed(&step.report(slot))?;
                    thread::sleep(RAMP_STEP);
                }
                vhid.feed(&PadState::NEUTRAL.report(slot))?;
                eprintln!("{}", strings::input_pad_pressed(hold_ms));
            }
            Command::Unknown(text) => {
                eprintln!(
                    "{}",
                    strings::input_pad_unknown(&text, &padmap::NAMES.join(" "))
                );
            }
        }
    }
    Ok(())
}

/// Creates the virtual Xbox pad node of `slot` (spec 0016). Admin.
fn pad_add(slot: u8) -> Result<(), SvcError> {
    let message = match pads::add(&mut WindowsPadHost, slot)? {
        AddOutcome::Added => strings::pad_added(slot),
        AddOutcome::Replaced => strings::pad_replaced(slot),
    };
    eprintln!("{message}");
    Ok(())
}

fn pad_remove(slot: u8) -> Result<(), SvcError> {
    match pads::remove(&mut WindowsPadHost, slot)? {
        RemoveOutcome::Removed => eprintln!("{}", strings::pad_removed(slot)),
        RemoveOutcome::NoDevice => eprintln!("{}", strings::pad_absent(slot)),
        RemoveOutcome::Held { app } => return Err(SvcError::DeviceHeld(app)),
    }
    Ok(())
}
