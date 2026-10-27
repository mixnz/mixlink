//! `mixlink-svc`: the host service (ADR-0005). Runs under the service manager, or with `--console`
//! for development; `install` / `uninstall` register it.

mod args;
mod capture;
mod capture_args;
mod discovery;
mod display;
mod drivers;
mod error;
mod host_config;
mod identity;
#[cfg(windows)]
mod input;
mod input_args;
mod json_file;
mod log_setup;
mod net_check;
mod pipe;
mod platform;
#[cfg(windows)]
mod run;
mod secrets;
#[cfg(windows)]
mod session_listen;
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
            // Ports as the service reads them now; changing them in host.json needs `install` again.
            let program_data = platform::windows::drivers_host::program_data();
            let net = host_config::load(&host_config::default_path(&program_data)).net;
            platform::windows::firewall::add_rule(&std::env::current_exe()?, &net)?;
            eprintln!("{}", strings::FIREWALL_ADDED);
            Ok(())
        }
        Mode::Uninstall => {
            let message = match platform::windows::install::uninstall()? {
                UninstallOutcome::Removed => strings::REMOVED,
                UninstallOutcome::NotInstalled => strings::NOT_INSTALLED,
            };
            eprintln!("{message}");
            platform::windows::firewall::remove_rule()?;
            eprintln!("{}", strings::FIREWALL_REMOVED);
            Ok(())
        }
        Mode::Drivers(cmd) => drivers::run(&cmd),
        Mode::Display(cmd) => {
            log_setup::init_console_log();
            display::commands::run(&cmd)
        }
        Mode::Input(cmd) => {
            log_setup::init_console_log();
            input::commands::run(&cmd)
        }
        Mode::Capture(cmd) => {
            log_setup::init_console_log();
            capture::run(&cmd)
        }
        Mode::DisplayAgent { input, output } => {
            platform::windows::display_agent::serve_one(&input, &output)
        }
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
    run::run(&stop, &host_run_options(pipe), || {
        log::info!("running in console mode; press Ctrl+C to stop");
    })
}

/// Options for service and console mode from `%ProgramData%\MixLink\host.json` (defaults when
/// missing or invalid) and the host identity. No identity → no discovery (logged).
#[cfg(windows)]
pub fn host_run_options(pipe: String) -> run::RunOptions {
    let program_data = platform::windows::drivers_host::program_data();
    let config = host_config::load(&host_config::default_path(&program_data));
    let id = identity::load_or_create(
        &identity::default_path(&program_data),
        platform::windows::random::random_device_id,
    );
    let (discovery, session) = match id {
        Ok(id) => (
            Some(discovery::settings(&config, id)),
            session_settings(&program_data, &config, id),
        ),
        Err(error) => {
            log::error!("host id unavailable, LAN discovery and pairing off: {error}");
            (None, None)
        }
    };
    run::RunOptions {
        pipe,
        display: Some(config.display),
        input: true,
        discovery,
        session,
    }
}

/// Secures the `secrets` folder, then loads or makes the host key. Any failure → no pairing (logged):
/// keys are never written to a folder other users could read.
#[cfg(windows)]
fn session_settings(
    program_data: &std::path::Path,
    config: &mixlink_core::config::HostConfig,
    host: mixlink_core::ids::DeviceId,
) -> Option<session_listen::SessionSettings> {
    let dir = secrets::dir(program_data);
    let key = platform::windows::dir_security::secure_dir(&dir).and_then(|()| {
        secrets::load_or_create_key(&dir, || {
            mixlink_net::keys::StaticKeyPair::generate()
                .map_err(|error| SvcError::Session(error.to_string()))
        })
    });
    match key {
        Ok(keys) => Some(session_listen::SessionSettings {
            net: config.net.clone(),
            video: config.video.clone(),
            host,
            keys,
            secrets: dir,
        }),
        Err(error) => {
            log::error!("host key unavailable, pairing off: {error}");
            None
        }
    }
}
