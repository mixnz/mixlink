//! Spike S2b helper for `run-case.ps1` (spec 0005). Runs in the user's session.
//! - `s2-ctl check`: virtual monitor must be the main display at 120 Hz (exit 1 otherwise).
//! - `s2-ctl cycle`: virtual monitor 120 -> 60 -> 120 Hz, so the driver gets a new swapchain and
//!   re-reads its lever mask.
//! - `s2-ctl pool <out-file> <frames.csv>...`: pooled stats over rounds; each round's `cond` lines are
//!   read from its `...-summary.txt` next to the CSV.

use std::fs;
use std::process::ExitCode;
use std::time::Duration;

use s2_iddcx_frames::display::{
    REQUIRED_HZ, check_verdict, main_display, set_refresh, virtual_gdi_name,
};
use s2_iddcx_frames::pool::{RoundInput, pool};
use s2_iddcx_frames::util::BoxResult;

const CYCLE_STEP: Duration = Duration::from_secs(1);

fn check() -> BoxResult<()> {
    let (main, hz) = main_display().ok_or("no main display found")?;
    let line = check_verdict(&main, hz, virtual_gdi_name().as_deref())?;
    println!("{line}");
    Ok(())
}

fn cycle() -> BoxResult<()> {
    let gdi = virtual_gdi_name().ok_or("virtual monitor not found")?;
    set_refresh(&gdi, 60)?;
    std::thread::sleep(CYCLE_STEP);
    set_refresh(&gdi, REQUIRED_HZ)?;
    std::thread::sleep(CYCLE_STEP);
    println!("{gdi}: 120 -> 60 -> 120 Hz done");
    Ok(())
}

fn pool_files(out: &str, csvs: &[String]) -> BoxResult<()> {
    let mut rounds = Vec::with_capacity(csvs.len());
    for path in csvs {
        let summary = path.replace("-frames.csv", "-summary.txt");
        let cond = fs::read_to_string(&summary)
            .map_err(|e| format!("{summary}: {e}"))?
            .lines()
            .filter(|l| l.starts_with("cond "))
            .map(str::to_string)
            .collect();
        let name = std::path::Path::new(path)
            .file_name()
            .map_or_else(|| path.clone(), |n| n.to_string_lossy().into_owned());
        rounds.push(RoundInput {
            name,
            csv: fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?,
            cond,
        });
    }
    let text = pool(&rounds)?;
    fs::write(out, &text)?;
    print!("{text}");
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let result = match args.get(1).map(String::as_str) {
        Some("check") => check(),
        Some("cycle") => cycle(),
        Some("pool") if args.len() >= 4 => pool_files(&args[2], &args[3..]),
        _ => Err("usage: s2-ctl check | cycle | pool <out-file> <frames.csv>...".into()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("s2-ctl: {e}");
            ExitCode::FAILURE
        }
    }
}
