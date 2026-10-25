//! Command line: which mode the process runs in.

use std::path::PathBuf;

use mixlink_core::ids::DeviceId;
use mixlink_core::svc_ipc::PIPE_NAME;
use mixlink_proto::control::DisplayInfo;

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
    Display(DisplayCmd),
}

/// `display ...` subcommands (spec 0015): developer checks of the virtual display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DisplayCmd {
    Status,
    /// Adds a monitor for `client` at `mode`, waits for Enter / Ctrl+C, removes it.
    Try {
        client: DeviceId,
        mode: DisplayInfo,
        topology: TryTopology,
    },
    /// Hidden: checks the display layout calls from this session (spec 0015, Session 0 check).
    Probe {
        out: PathBuf,
    },
    /// Hidden: sends malformed `ADD_MONITOR` requests; each must be refused.
    RejectCheck,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TryTopology {
    /// Monitor only; the screen layout is left alone.
    Keep,
}

/// Mode from the command line: dpi 96, SDR.
const TRY_DPI: u16 = 96;

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
     mixlink-svc drivers [install [--from <dir>] | uninstall | status]\n       \
     mixlink-svc display [status | try --client <32 hex> --mode <W>x<H>@<Hz> --topology keep]";

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
        ["display", "status"] => Ok(Mode::Display(DisplayCmd::Status)),
        [
            "display",
            "try",
            "--client",
            client,
            "--mode",
            mode,
            "--topology",
            topology,
        ] => Ok(Mode::Display(DisplayCmd::Try {
            client: client
                .parse()
                .map_err(|_| format!("--client needs 32 hex digits, got {client}"))?,
            mode: parse_mode(mode)?,
            topology: parse_topology(topology)?,
        })),
        ["display", "probe", "--out", out] => Ok(Mode::Display(DisplayCmd::Probe {
            out: PathBuf::from(out),
        })),
        ["display", "reject-check"] => Ok(Mode::Display(DisplayCmd::RejectCheck)),
        _ => Err(format!("unknown arguments: {}", args.join(" "))),
    }
}

fn parse_topology(text: &str) -> Result<TryTopology, String> {
    match text {
        "keep" => Ok(TryTopology::Keep),
        _ => Err(format!("--topology must be keep, got {text}")),
    }
}

/// `WxH@Hz`, refresh with up to three decimals (`59.94`), stored in millihertz.
fn parse_mode(text: &str) -> Result<DisplayInfo, String> {
    let bad = || format!("--mode must look like 2560x1440@120, got {text}");
    let (size, refresh) = text.split_once('@').ok_or_else(bad)?;
    let (width, height) = size.split_once('x').ok_or_else(bad)?;
    let width: u32 = width.parse().map_err(|_| bad())?;
    let height: u32 = height.parse().map_err(|_| bad())?;
    let (whole, fraction) = refresh.split_once('.').unwrap_or((refresh, ""));
    if fraction.len() > 3 || !fraction.bytes().all(|b| b.is_ascii_digit()) {
        return Err(bad());
    }
    let whole: u32 = whole.parse().map_err(|_| bad())?;
    let fraction: u32 = format!("{fraction:0<3}").parse().map_err(|_| bad())?;
    let refresh_mhz = whole
        .checked_mul(1000)
        .and_then(|mhz| mhz.checked_add(fraction))
        .ok_or_else(bad)?;
    if width == 0 || height == 0 || refresh_mhz == 0 {
        return Err(bad());
    }
    Ok(DisplayInfo {
        width,
        height,
        refresh_mhz,
        hdr: false,
        dpi: TRY_DPI,
    })
}

#[cfg(test)]
#[path = "args_tests.rs"]
mod tests;
