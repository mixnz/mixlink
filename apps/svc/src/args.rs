//! Command line: which mode the process runs in.

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
}

pub const USAGE: &str = "usage: mixlink-svc [--console [--pipe <name>] | install | uninstall]";

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
        _ => Err(format!("unknown arguments: {}", args.join(" "))),
    }
}

#[cfg(test)]
#[path = "args_tests.rs"]
mod tests;
