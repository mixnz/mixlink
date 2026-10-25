//! Command line: which mode the process runs in.

use std::path::PathBuf;

use mixlink_core::svc_ipc::PIPE_NAME;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    /// Started by the service manager.
    Service,
    /// Development: same work as the service, in the current session.
    Console {
        pipe: String,
    },
    Install,
    Uninstall,
    Drivers(DriversCmd),
}

/// `drivers ...` subcommands (spec 0014).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DriversCmd {
    /// `from`: folder with `<name>\[pkg\]` packages instead of `<exe dir>\drivers`.
    Install {
        from: Option<PathBuf>,
    },
    Uninstall,
    Status,
}

pub const USAGE: &str = "usage: mixlink-svc [--console [--pipe <name>] | install | uninstall]\n       \
     mixlink-svc drivers [install [--from <dir>] | uninstall | status]";

/// Parses the arguments after the program name.
pub fn parse_args<I: IntoIterator<Item = String>>(args: I) -> Result<Mode, String> {
    let args: Vec<String> = args.into_iter().collect();
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        [] => Ok(Mode::Service),
        ["--console"] => Ok(Mode::Console {
            pipe: PIPE_NAME.to_owned(),
        }),
        ["--console", "--pipe", name] => Ok(Mode::Console {
            pipe: (*name).to_owned(),
        }),
        ["install"] => Ok(Mode::Install),
        ["uninstall"] => Ok(Mode::Uninstall),
        ["drivers", "install"] => Ok(Mode::Drivers(DriversCmd::Install { from: None })),
        ["drivers", "install", "--from", dir] => Ok(Mode::Drivers(DriversCmd::Install {
            from: Some(PathBuf::from(dir)),
        })),
        ["drivers", "uninstall"] => Ok(Mode::Drivers(DriversCmd::Uninstall)),
        ["drivers", "status"] => Ok(Mode::Drivers(DriversCmd::Status)),
        _ => Err(format!("unknown arguments: {}", args.join(" "))),
    }
}

#[cfg(test)]
#[path = "args_tests.rs"]
mod tests;
