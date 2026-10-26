//! The `input ...` developer commands (spec 0016).

use std::io::{BufRead, Write};
use std::sync::Arc;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use mixlink_core::clock;
use mixlink_input::hid::gamepad::PadState;
use mixlink_input::hid::keyboard::KeyboardState;
use mixlink_input::platform::windows::vhid::VHid;

use super::open_feeder;
use super::padmap::{self, Command};
use super::probe::{ProbeDevice, assign_arrivals, period, report_for, summary};
use super::script::{Script, play, steps};
use super::status::format_status;
use crate::args::InputCmd;
use crate::error::SvcError;
use crate::platform::windows::console::install_ctrl_handler;
use crate::platform::windows::raw_input;
use crate::stop::StopSignal;
use crate::strings;

/// `input probe`: events 2 ms apart, as in spec 0006.
const EVENT_SPACING_US: u64 = 2_000;
/// Before the first and after the last event, so nothing is cut off.
const SETTLE: Duration = Duration::from_millis(200);
const RECEIVER_START: Duration = Duration::from_secs(5);
/// `input pad` `prime`: time per nudge.
const PRIME_STEP: Duration = Duration::from_millis(100);
/// `input pad`: time per ramp step (10 steps ≈ 0.2 s, like a quick real pull).
const RAMP_STEP: Duration = Duration::from_millis(20);

pub fn run(cmd: &InputCmd) -> Result<(), SvcError> {
    match cmd {
        InputCmd::Status => status(),
        InputCmd::Pad { delay_s } => pad_interactive(*delay_s),
        InputCmd::Try { script, delay_s } => try_script(*script, *delay_s),
        InputCmd::Probe { device, count } => probe(*device, *count),
    }
}

fn status() -> Result<(), SvcError> {
    let reply = VHid::open()?.query()?;
    std::io::stdout().write_all(format_status(&reply).as_bytes())?;
    Ok(())
}

/// Plays `script` as the driver's feeder. Fails with the "in use" message while the service holds
/// the driver. Ctrl+C stops early; closing the handle releases whatever was held.
fn try_script(script: Script, delay_s: u32) -> Result<(), SvcError> {
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
    play(&steps(script), &mut vhid, &mut wait)?;
    if script == Script::Hold {
        eprintln!("{}", strings::INPUT_TRY_HOLD);
        stop.wait();
    } else {
        eprintln!("{}", strings::INPUT_TRY_DONE);
    }
    Ok(())
}

/// Feeds `count` events 2 ms apart and times their arrival in Raw Input (spec 0016, same method as
/// spec 0006). Run elevated, with the service stopped, without touching the real mouse.
fn probe(device: ProbeDevice, count: u32) -> Result<(), SvcError> {
    let vhid = open_feeder()?;
    let n = usize::try_from(count).unwrap_or(0);
    let (ready_tx, ready_rx) = mpsc::channel();
    let receiver = thread::Builder::new()
        .name("input-probe-recv".into())
        .spawn(move || raw_input::receive(device, n * 2 + 16, &ready_tx))?;
    let Ok(thread_id) = ready_rx.recv_timeout(RECEIVER_START) else {
        return match receiver.join() {
            Ok(Err(error)) => Err(error),
            Ok(Ok(_)) | Err(_) => Err(SvcError::Io(std::io::Error::other(
                "Raw Input receiver did not start",
            ))),
        };
    };
    thread::sleep(SETTLE);
    let mut sent = vec![0i64; n];
    let fed = feed_events(&vhid, device, &mut sent);
    thread::sleep(SETTLE);
    raw_input::quit(thread_id);
    let arrivals = match receiver.join() {
        Ok(result) => result?,
        Err(_) => {
            return Err(SvcError::Io(std::io::Error::other(
                "Raw Input receiver panicked",
            )));
        }
    };
    fed?;
    let recv = assign_arrivals(&sent, period(device), &arrivals);
    let freq = i64::try_from(clock::ticks_per_sec()).unwrap_or(i64::MAX);
    let s = summary(&sent, &recv, freq);
    let line = strings::probe_summary(
        device_name(device),
        s.p50_us,
        s.p99_us,
        s.max_us,
        s.sent,
        s.received,
        s.lost,
    );
    writeln!(std::io::stdout(), "{line}")?;
    Ok(())
}

/// One event every `EVENT_SPACING`, QPC taken right before each feed.
fn feed_events(vhid: &VHid, device: ProbeDevice, sent: &mut [i64]) -> Result<(), SvcError> {
    let spacing = clock::ticks_per_sec() * EVENT_SPACING_US / 1_000_000;
    let mut keyboard = KeyboardState::default();
    let mut next = clock::now().raw();
    for (i, slot) in (0u32..).zip(sent.iter_mut()) {
        while clock::now().raw() < next {
            thread::yield_now();
        }
        let report = report_for(device, i, &mut keyboard);
        *slot = i64::try_from(clock::now().raw()).unwrap_or(i64::MAX);
        vhid.feed(&report)?;
        next += spacing;
    }
    Ok(())
}

fn device_name(device: ProbeDevice) -> &'static str {
    match device {
        ProbeDevice::Kbd => "keyboard",
        ProbeDevice::Rel => "mouse relative",
        ProbeDevice::Abs => "mouse absolute",
        ProbeDevice::Pad => "gamepad",
    }
}

/// Reads `<control> [hold_ms]` lines from stdin and presses that control, for mapping wizards
/// (Steam "Begin Setup"). The pad is centred after each press and when the command ends.
fn pad_interactive(delay_s: u32) -> Result<(), SvcError> {
    let vhid = open_feeder()?;
    eprintln!("{}", strings::input_pad_help(&padmap::NAMES.join(" ")));
    for line in std::io::stdin().lock().lines() {
        match padmap::parse_command(&line?) {
            Command::Quit => break,
            Command::Skip => {}
            Command::Prime => {
                for state in padmap::prime_steps() {
                    vhid.feed(&state.report())?;
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
                    vhid.feed(&step.report())?;
                    thread::sleep(RAMP_STEP);
                }
                thread::sleep(Duration::from_millis(u64::from(hold_ms)));
                for step in ramp.iter().rev().skip(1) {
                    vhid.feed(&step.report())?;
                    thread::sleep(RAMP_STEP);
                }
                vhid.feed(&PadState::NEUTRAL.report())?;
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
