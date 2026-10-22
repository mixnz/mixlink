//! Receiver service `MixLinkS2` (LocalSystem, Session 0). Installed by `install-svc.ps1`.
//! Args (service ImagePath): `--mode timing|png|dda --case X [--minutes N] [--conditions <file>]`.
//! - timing: IddCx latency run (case A); png: a PNG every 5 s (cases C-F); dda: runs `s2-dda` in the
//!   console session as SYSTEM (case B). The service stops itself after `--minutes` (default 10).
//! - `--conditions`: run conditions from `run-case.ps1`, copied into the summary as `cond` lines.

use std::sync::atomic::AtomicBool;

use s2_iddcx_frames::console_spawn::run_in_console;
use s2_iddcx_frames::log::RunLog;
use s2_iddcx_frames::receiver::{self, Mode};
use s2_iddcx_frames::report::conditions_from_args;
use s2_iddcx_frames::service;
use s2_iddcx_frames::util::{BoxResult, arg_u64, arg_value};

fn work(args: &[String], stop: &AtomicBool) -> BoxResult<()> {
    let mode = arg_value(args, "--mode").unwrap_or("timing");
    let case = arg_value(args, "--case").unwrap_or("dev");
    let minutes = arg_u64(args, "--minutes", 10)?;
    let mut log = RunLog::create(case, "svc")?;
    log.line(&format!("start mode {mode} case {case} minutes {minutes}"));
    let conditions = conditions_from_args(args);
    let result = match mode {
        "timing" => receiver::run(Mode::Timing, case, minutes, &conditions, stop, &mut log),
        "png" => receiver::run(Mode::Png, case, minutes, &conditions, stop, &mut log),
        "dda" => {
            let exe = std::env::current_exe()?.with_file_name("s2-dda.exe");
            let mut dda_args = format!("--case {case} --minutes {minutes}");
            if let Some(path) = arg_value(args, "--conditions") {
                dda_args.push_str(&format!(" --conditions \"{path}\""));
            }
            run_in_console(&exe.to_string_lossy(), &dda_args, stop).map(|(code, session)| {
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
    arg_u64(&args, "--minutes", 10)?; // reject a bad value before the SCM starts the work
    service::run(Box::new(move |stop| {
        let _ = work(&args, stop);
    }))
}
