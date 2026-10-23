//! s3-feed: scripted input for the game checks (spec 0006 cases C, F-K).
//!   s3-feed <vhid|sendinput|vigem> <scenario> [--delay <s>]
//!   s3-feed vhid --raw <hex>        one raw control output report (driver validation check)
//!   s3-feed <vhid|vigem> --pad-interactive   press one named pad control per typed line

use std::io::BufRead;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use s3_vhid::feed::{Backend, Feeder, parse_hex};
use s3_vhid::input::Input;
use s3_vhid::log;
use s3_vhid::padmap::{self, Command};
use s3_vhid::platform::{self, hid::VhidFeeder};
use s3_vhid::report::PadState;
use s3_vhid::scenario::{Scenario, steps};

const USAGE: &str = "usage: s3-feed <vhid|sendinput|vigem> <Walk|Look|Menu|PadWalk|PadLook|PadMenu|UacTab|LockSpace> [--delay <s>]\n       s3-feed vhid --raw <hex>\n       s3-feed <vhid|vigem> --pad-interactive";
/// Time each prime step is held, long enough for a reader to see the change.
const PRIME_STEP: Duration = Duration::from_millis(100);

/// Reads commands from stdin (`padmap::parse_command`): presses a control for its hold time,
/// or primes the analogs, then centers the pad.
fn pad_interactive(feeder: &mut dyn Feeder, lines: &mut Vec<String>) -> Result<(), String> {
    println!("controls: {}", padmap::NAMES.join(" "));
    println!("type '<control> [hold_ms]' + Enter to press it (default 500 ms);");
    println!("'prime' nudges every trigger/stick once; 'skip' does nothing; 'q' quits");
    let send =
        |f: &mut dyn Feeder, state: PadState| f.send(&Input::Pad(state)).map_err(|e| e.to_string());
    send(feeder, PadState::CENTERED)?;
    for line in std::io::stdin().lock().lines() {
        let line = line.map_err(|e| e.to_string())?;
        match padmap::parse_command(&line) {
            Command::Quit => break,
            Command::Skip => println!("(nothing pressed)"),
            Command::Prime => {
                for state in padmap::prime_steps() {
                    send(feeder, state)?;
                    std::thread::sleep(PRIME_STEP);
                }
                println!("primed triggers and sticks");
                lines.push("primed".into());
            }
            Command::Press { state, hold_ms } => {
                send(feeder, state)?;
                std::thread::sleep(Duration::from_millis(u64::from(hold_ms)));
                send(feeder, PadState::CENTERED)?;
                println!("pressed '{}' for {hold_ms} ms", line.trim());
                lines.push(format!("pressed '{}' for {hold_ms} ms", line.trim()));
            }
            Command::Unknown(text) => {
                println!("unknown '{text}'; controls: {}", padmap::NAMES.join(" "))
            }
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut lines = vec![format!("s3-feed {}", args.join(" "))];
    let result = run(&args, &mut lines);
    if let Err(e) = &result {
        lines.push(format!("ERROR {e}"));
    }
    for line in &lines {
        println!("{line}");
    }
    let name = log::run_name(&format!(
        "feed-{}",
        args.join("-").replace(['\\', '/', ' '], "_")
    ));
    match log::write_text(&name, &lines) {
        Ok(path) => println!("Log: {}", path.display()),
        Err(e) => eprintln!("log not written: {e}"),
    }
    if result.is_ok() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn run(args: &[String], lines: &mut Vec<String>) -> Result<(), String> {
    let backend = args.first().and_then(|a| Backend::parse(a)).ok_or(USAGE)?;
    if args.get(1).map(String::as_str) == Some("--raw") {
        let bytes = args.get(2).and_then(|h| parse_hex(h)).ok_or(USAGE)?;
        if backend != Backend::Vhid {
            return Err("--raw needs the vhid backend".into());
        }
        let mut feeder = VhidFeeder::open().map_err(|e| e.to_string())?;
        let result = feeder.write_raw(&bytes);
        lines.push(format!(
            "raw write of {} bytes: {:?}",
            bytes.len(),
            result.as_ref().map_err(ToString::to_string)
        ));
        return Ok(());
    }
    if args.get(1).map(String::as_str) == Some("--pad-interactive") {
        if backend == Backend::SendInput {
            return Err("SendInput has no gamepad".into());
        }
        let mut feeder = platform::open_feeder(backend).map_err(|e| e.to_string())?;
        return pad_interactive(feeder.as_mut(), lines);
    }
    let scenario = args.get(1).and_then(|s| Scenario::parse(s)).ok_or(USAGE)?;
    let delay = match args.iter().position(|a| a == "--delay") {
        Some(p) => args
            .get(p + 1)
            .and_then(|d| d.parse::<u64>().ok())
            .ok_or(USAGE)?,
        None => 5,
    };

    let mut feeder = platform::open_feeder(backend).map_err(|e| e.to_string())?;
    for left in (1..=delay).rev() {
        println!("starting {scenario:?} in {left} s ...");
        std::thread::sleep(Duration::from_secs(1));
    }
    let start = Instant::now();
    for step in steps(scenario) {
        let due = Duration::from_millis(u64::from(step.at_ms));
        if let Some(wait) = due.checked_sub(start.elapsed()) {
            std::thread::sleep(wait);
        }
        feeder
            .send(&step.action)
            .map_err(|e| format!("at {} ms, {:?}: {e}", step.at_ms, step.action))?;
    }
    lines.push(format!(
        "done: {scenario:?} via {backend:?} in {} ms",
        start.elapsed().as_millis()
    ));
    Ok(())
}
