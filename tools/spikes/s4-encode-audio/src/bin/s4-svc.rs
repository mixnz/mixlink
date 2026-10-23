//! `s4-svc [--console] --case X --side A|B|C <encode|audio|both> [options]`: runs one job and exits.
//! Without `--console` it must be started by the service manager: service `MixLinkS4` (LocalSystem,
//! Session 0), args from its ImagePath (`install-svc.ps1`, `run-case.ps1`); it stops itself when done.
//! `both` runs the encode job on a second thread next to the audio job. `--in-console` (service only):
//! the service starts the job as SYSTEM in the active console session instead (side C).

use std::sync::atomic::AtomicBool;

use s4_encode_audio::args::{AudioArgs, EncodeArgs, Job, RunArgs, parse};
use s4_encode_audio::console_spawn::run_in_console;
use s4_encode_audio::log::RunLog;
use s4_encode_audio::util::{BoxResult, RunFiles};
use s4_encode_audio::{audio_job, encode_job, service};

fn encode(
    a: &EncodeArgs,
    files: &RunFiles,
    stop: &AtomicBool,
    log: &mut RunLog,
) -> BoxResult<String> {
    let r = encode_job::run(a, files, stop, log)?;
    files.write_result("encode", &r.summary, &r.latency_us)?;
    Ok(r.summary)
}

fn audio(
    a: &AudioArgs,
    files: &RunFiles,
    stop: &AtomicBool,
    log: &mut RunLog,
) -> BoxResult<String> {
    let r = audio_job::run(a, files, stop, log)?;
    // Pooled and compared A vs B: packet interval (the offset is in the summary).
    files.write_result("audio", &r.summary, &r.interval_us)?;
    Ok(r.summary)
}

fn both(
    e: &EncodeArgs,
    a: &AudioArgs,
    run: &RunArgs,
    stop: &AtomicBool,
    log: &mut RunLog,
) -> BoxResult<String> {
    let files = RunFiles::new(&run.case, run.side);
    std::thread::scope(|s| {
        let enc = s.spawn(|| -> Result<String, String> {
            let mut elog = RunLog::create(&run.case, &format!("{}-svc-encode", run.side))
                .map_err(|e| e.to_string())?;
            let r = encode(e, &files, stop, &mut elog);
            if let Err(err) = &r {
                elog.line(&format!("error: {err}"));
            }
            r.map_err(|e| e.to_string())
        });
        let audio_summary = audio(a, &files, stop, log);
        let enc_summary = enc.join().map_err(|_| "encode thread panicked")?;
        Ok(format!(
            "{}{}",
            enc_summary.unwrap_or_else(|e| format!("encode error: {e}\n")),
            audio_summary.unwrap_or_else(|e| format!("audio error: {e}\n"))
        ))
    })
}

fn run_job(run: &RunArgs, stop: &AtomicBool) -> BoxResult<()> {
    let files = RunFiles::new(&run.case, run.side);
    let mut log = RunLog::create(&run.case, &format!("{}-svc", run.side))?;
    log.line(&format!("start {run:?}"));
    if run.delay_s > 0 {
        std::thread::sleep(std::time::Duration::from_secs(u64::from(run.delay_s)));
    }
    let result = match &run.job {
        Job::Encode(e) => encode(e, &files, stop, &mut log),
        Job::Audio(a) => audio(a, &files, stop, &mut log),
        Job::Both(e, a) => both(e, a, run, stop, &mut log),
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
        let _ = if run.in_console {
            spawn_in_console(&args, &run, stop)
        } else {
            run_job(&run, stop)
        };
    }))
}

/// Side C (ADR-0005 fallback layout): the same job as SYSTEM in the console session, waited for.
fn spawn_in_console(args: &[String], run: &RunArgs, stop: &AtomicBool) -> BoxResult<()> {
    let mut log = RunLog::create(&run.case, &format!("{}-svc-spawn", run.side))?;
    let child: Vec<&str> = args
        .iter()
        .map(String::as_str)
        .filter(|a| *a != "--in-console")
        .collect();
    let exe = std::env::current_exe()?;
    let child_args = format!("--console {}", child.join(" "));
    log.line(&format!("spawning in console session: {child_args}"));
    match run_in_console(&exe.to_string_lossy(), &child_args, stop) {
        Ok((code, session)) => {
            log.line(&format!("child exited {code} in session {session}"));
            Ok(())
        }
        Err(e) => {
            log.line(&format!("spawn failed: {e}"));
            Err(e)
        }
    }
}
