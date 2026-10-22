//! Receiver service `MixLinkS2` (LocalSystem, Session 0). Installed by `install-svc.ps1`.
//! Args (service ImagePath): `--mode timing|png|dda --case X [--minutes N]`.
//! - timing: IddCx latency run (case A); png: a PNG every 5 s (cases C-F); dda: runs `s2-dda` in the
//!   console session as SYSTEM (case B). The service stops itself after `--minutes` (default 10).

use std::sync::atomic::AtomicBool;

use s2_iddcx_frames::console_spawn::run_in_console;
use s2_iddcx_frames::log::RunLog;
use s2_iddcx_frames::receiver::{self, Mode};
use s2_iddcx_frames::service;
use s2_iddcx_frames::util::{BoxResult, arg_u64, arg_value};

fn work(mode: &str, case: &str, minutes: u64, stop: &AtomicBool) -> BoxResult<()> {
    let mut log = RunLog::create(case, "svc")?;
    log.line(&format!("start mode {mode} case {case} minutes {minutes}"));
    let result = match mode {
        "timing" => receiver::run(Mode::Timing, case, minutes, stop, &mut log),
        "png" => receiver::run(Mode::Png, case, minutes, stop, &mut log),
        "dda" => {
            let exe = std::env::current_exe()?.with_file_name("s2-dda.exe");
            let args = format!("--case {case} --minutes {minutes}");
            run_in_console(&exe.to_string_lossy(), &args, stop).map(|(code, session)| {
                log.line(&format!("s2-dda exited {code} in session {session}"));
            })
        }
        other => Err(format!("unknown mode {other}").into()),
    };
    if let Err(e) = &result {
        log.line(&format!("error: {e}"));
    }
    log.line("end");
    result
}

fn main() -> BoxResult<()> {
    let args: Vec<String> = std::env::args().collect();
    let mode = arg_value(&args, "--mode").unwrap_or("timing").to_string();
    let case = arg_value(&args, "--case").unwrap_or("dev").to_string();
    let minutes = arg_u64(&args, "--minutes", 10)?;
    service::run(Box::new(move |stop| {
        let _ = work(&mode, &case, minutes, stop);
    }))
}
