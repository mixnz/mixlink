//! `s4-svc [--console] --case X --side A|B|C <encode|audio|both> [options]`: runs one job and exits.
//! Without `--console` it must be started by the service manager: service `MixLinkS4` (LocalSystem,
//! Session 0), args from its ImagePath (`install-svc.ps1`, `run-case.ps1`); it stops itself when done.

use std::sync::atomic::AtomicBool;

use s4_encode_audio::args::{Job, RunArgs, parse};
use s4_encode_audio::encode_job;
use s4_encode_audio::log::RunLog;
use s4_encode_audio::service;
use s4_encode_audio::util::{BoxResult, RunFiles};

fn run_job(run: &RunArgs, stop: &AtomicBool) -> BoxResult<()> {
    let files = RunFiles::new(&run.case, run.side);
    let mut log = RunLog::create(&run.case, &format!("{}-svc", run.side))?;
    log.line(&format!("start {run:?}"));
    if run.delay_s > 0 {
        std::thread::sleep(std::time::Duration::from_secs(u64::from(run.delay_s)));
    }
    let result = match &run.job {
        Job::Encode(a) => encode_job::run(a, &files, stop, &mut log).and_then(|r| {
            files
                .write_result("encode", &r.summary, &r.latency_us)
                .map(|_| r.summary)
        }),
        _ => Err("audio jobs come in Task 4".into()),
    };
    match result {
        Ok(summary) => {
            log.line("done");
            print!("{summary}");
            Ok(())
        }
        Err(e) => {
            log.line(&format!("error: {e}"));
            Err(e)
        }
    }
}

fn main() -> BoxResult<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let run = parse(&args)?;
    if run.console {
        return run_job(&run, &AtomicBool::new(false));
    }
    service::run(Box::new(move |stop| {
        // Errors are already in the run log; the service just stops.
        let _ = run_job(&run, stop);
    }))
}
