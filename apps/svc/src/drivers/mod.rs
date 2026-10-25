//! Driver install tooling (spec 0014): per-machine signing and install of `vdisplay` + `vhid`.

pub mod flow;
pub mod package;
pub mod state;
pub mod status;

#[cfg(windows)]
use crate::args::DriversCmd;
#[cfg(windows)]
use crate::error::SvcError;

/// Runs a `drivers ...` command. Messages go to stderr like the service commands; `status` to stdout.
#[cfg(windows)]
pub fn run(cmd: &DriversCmd) -> Result<(), SvcError> {
    use std::io::Write;
    use std::path::PathBuf;

    use crate::platform::windows::drivers_host::{WindowsDriverHost, query_status};
    use crate::strings;
    use flow::{InstallOutcome, UninstallOutcome};
    use package::{DRIVERS, DriverSpec, resolve_source};

    match cmd {
        DriversCmd::Install { from } => {
            let mut host = WindowsDriverHost::new()?;
            let exe = std::env::current_exe()?;
            let exe_dir = exe.parent().unwrap_or(&exe);
            let sources = DRIVERS
                .iter()
                .map(|spec| Ok((spec, resolve_source(exe_dir, from.as_deref(), spec)?)))
                .collect::<Result<Vec<(&DriverSpec, PathBuf)>, SvcError>>()?;
            let message = match flow::install(&mut host, &sources)? {
                InstallOutcome::Installed => strings::DRIVERS_INSTALLED,
                InstallOutcome::Updated => strings::DRIVERS_UPDATED,
                InstallOutcome::Held { app } => return Err(SvcError::DeviceHeld(app)),
            };
            eprintln!("{message}");
        }
        DriversCmd::Uninstall => {
            let mut host = WindowsDriverHost::new()?;
            let message = match flow::uninstall(&mut host, &DRIVERS)? {
                UninstallOutcome::Removed => strings::DRIVERS_REMOVED,
                UninstallOutcome::NothingInstalled => strings::DRIVERS_NOT_INSTALLED,
                UninstallOutcome::Held { app } => return Err(SvcError::DeviceHeld(app)),
            };
            eprintln!("{message}");
        }
        DriversCmd::Status => {
            let text = status::format_status(&query_status()?);
            std::io::stdout().write_all(text.as_bytes())?;
        }
    }
    Ok(())
}
