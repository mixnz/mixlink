//! s3-hook: are events flagged as injected? (spec 0006 case C)
//!   s3-hook [--seconds 30]   then run s3-feed in another console while it counts.

use std::process::ExitCode;
use std::time::Duration;

use s3_vhid::log;
use s3_vhid::platform::hook;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let seconds = match args.iter().position(|a| a == "--seconds") {
        Some(p) => match args.get(p + 1).and_then(|s| s.parse::<u64>().ok()) {
            Some(s) => s,
            None => {
                eprintln!("usage: s3-hook [--seconds N]");
                return ExitCode::FAILURE;
            }
        },
        None => 30,
    };
    println!("counting keyboard and mouse events for {seconds} s ...");
    let mut lines = vec![format!("s3-hook --seconds {seconds}")];
    let ok = match hook::count(Duration::from_secs(seconds)) {
        Ok(c) => {
            lines.push(format!(
                "keyboard: {} clean, {} injected",
                c.key_clean, c.key_injected
            ));
            lines.push(format!(
                "mouse: {} clean, {} injected",
                c.mouse_clean, c.mouse_injected
            ));
            true
        }
        Err(e) => {
            lines.push(format!("ERROR {e}"));
            false
        }
    };
    for line in &lines {
        println!("{line}");
    }
    match log::write_text(&log::run_name("hook"), &lines) {
        Ok(path) => println!("Log: {}", path.display()),
        Err(e) => eprintln!("log not written: {e}"),
    }
    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
