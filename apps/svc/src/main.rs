//! `mixlink-svc`: the host service (ADR-0005). Runs under the service manager, or with `--console`
//! for development; `install` / `uninstall` register it.

mod args;
mod drivers;
mod error;
mod log_setup;
mod pipe;
mod platform;
#[cfg(windows)]
mod run;
mod stop;
mod strings;

use std::process::ExitCode;

use crate::args::{Mode, USAGE, parse_args};
use crate::error::SvcError;
#[cfg(windows)]
use crate::platform::windows::install::{InstallOutcome, UninstallOutcome};

const EXIT_USAGE: u8 = 2;
/// `drivers uninstall`: a program holds a MixLink device open (spec 0014).
const EXIT_HELD: u8 = 3;

fn main() -> ExitCode {
    let mode = match parse_args(std::env::args().skip(1)) {
        Ok(mode) => mode,
        Err(message) => {
            eprintln!("{message}\n{USAGE}");
            return ExitCode::from(EXIT_USAGE);
        }
    };
    match run_mode(mode) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error @ SvcError::DeviceHeld(_)) => {
            eprintln!("{error}");
            ExitCode::from(EXIT_HELD)
        }
        Err(SvcError::Usage(message)) => {
            eprintln!("{message}");
            ExitCode::from(EXIT_USAGE)
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(windows)]
fn run_mode(mode: Mode) -> Result<(), SvcError> {
    match mode {
        Mode::Service => platform::windows::service::dispatch(),
        Mode::Console { pipe } => run_console(pipe),
        Mode::Install => {
            let message = match platform::windows::install::install()? {
                InstallOutcome::Created => strings::INSTALLED,
                InstallOutcome::Updated => strings::UPDATED,
            };
            eprintln!("{message}");
            Ok(())
        }
        Mode::Uninstall => {
            let message = match platform::windows::install::uninstall()? {
                UninstallOutcome::Removed => strings::REMOVED,
                UninstallOutcome::NotInstalled => strings::NOT_INSTALLED,
            };
            eprintln!("{message}");
            Ok(())
        }
        Mode::Drivers(cmd) => drivers::run(&cmd),
    }
}

#[cfg(not(windows))]
fn run_mode(_mode: Mode) -> Result<(), SvcError> {
    Err(SvcError::Io(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "mixlink-svc runs on Windows only",
    )))
}

#[cfg(windows)]
fn run_console(pipe: String) -> Result<(), SvcError> {
    use std::sync::Arc;

    log_setup::init_console_log();
    let stop = Arc::new(stop::StopSignal::new());
    platform::windows::console::install_ctrl_handler(Arc::clone(&stop))?;
    run::run(&stop, &run::RunOptions { pipe }, || {
        log::info!("running in console mode; press Ctrl+C to stop");
    })
}
