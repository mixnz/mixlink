//! `mixlink-svc`: the host service (ADR-0005). Runs under the service manager, or with `--console`
//! for development; `install` / `uninstall` register it.

mod args;
mod error;
mod log_setup;
mod pipe;
mod stop;

use std::process::ExitCode;

use crate::args::{Mode, USAGE, parse_args};
use crate::error::SvcError;

const EXIT_USAGE: u8 = 2;

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

fn run_mode(mode: Mode) -> Result<(), SvcError> {
    match mode {
        Mode::Service | Mode::Console { .. } | Mode::Install | Mode::Uninstall => {
            Err(SvcError::Usage("not implemented yet".to_owned()))
        }
    }
}
