//! Command line: which mode the process runs in.

use std::path::PathBuf;

use mixlink_core::config::Topology;
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
    Input(InputCmd),
    /// The display helper the service starts in the console session (spec 0015); not for people.
    DisplayAgent {
        input: PathBuf,
        output: PathBuf,
    },
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
    /// Monitor as main display with this layout, restored after.
    Apply(Topology),
}

/// Mode from the command line: dpi 96, SDR.
const TRY_DPI: u16 = 96;

/// `input ...` subcommands (spec 0016): developer checks of the virtual input driver.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputCmd {
    Status,
    /// Plays `script` after `delay_s` seconds (time to focus a game, UAC prompt or lock screen).
    Try {
        script: Script,
        delay_s: u32,
    },
}

/// Scripted input for `input try`, side-effect free (`input::script`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Script {
    Keys,
    /// F13..F22 down together for 1 s, then up (NKRO check).
    Chord,
    Mouse,
    Abs,
    Pad,
    /// Keys, Mouse, Abs, Pad in turn.
    All,
    /// F13 down and never released: the release-all check (kill the process, or Ctrl+C).
    Hold,
}

/// Longest `input try --delay`.
const MAX_TRY_DELAY_S: u32 = 600;

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
     mixlink-svc display [status | try --client <32 hex> --mode <W>x<H>@<Hz> --topology virtual-only|extend|keep]\n       \
     mixlink-svc input [status | try --script keys|chord|mouse|abs|pad|all|hold [--delay <s>]]";

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
        ["display-agent", "--in", input, "--out", output] => Ok(Mode::DisplayAgent {
            input: PathBuf::from(input),
            output: PathBuf::from(output),
        }),
        ["display", "reject-check"] => Ok(Mode::Display(DisplayCmd::RejectCheck)),
        ["input", "status"] => Ok(Mode::Input(InputCmd::Status)),
        ["input", "try", "--script", script] => Ok(Mode::Input(InputCmd::Try {
            script: parse_script(script)?,
            delay_s: 0,
        })),
        ["input", "try", "--script", script, "--delay", delay] => Ok(Mode::Input(InputCmd::Try {
            script: parse_script(script)?,
            delay_s: parse_delay(delay)?,
        })),
        _ => Err(format!("unknown arguments: {}", args.join(" "))),
    }
}

fn parse_script(text: &str) -> Result<Script, String> {
    match text {
        "keys" => Ok(Script::Keys),
        "chord" => Ok(Script::Chord),
        "mouse" => Ok(Script::Mouse),
        "abs" => Ok(Script::Abs),
        "pad" => Ok(Script::Pad),
        "all" => Ok(Script::All),
        "hold" => Ok(Script::Hold),
        _ => Err(format!(
            "--script must be keys, chord, mouse, abs, pad, all or hold, got {text}"
        )),
    }
}

fn parse_delay(text: &str) -> Result<u32, String> {
    text.parse::<u32>()
        .ok()
        .filter(|&s| s <= MAX_TRY_DELAY_S)
        .ok_or_else(|| format!("--delay must be 0..={MAX_TRY_DELAY_S} seconds, got {text}"))
}

fn parse_topology(text: &str) -> Result<TryTopology, String> {
    match text {
        "keep" => Ok(TryTopology::Keep),
        "virtual-only" => Ok(TryTopology::Apply(Topology::VirtualOnly)),
        "extend" => Ok(TryTopology::Apply(Topology::Extend)),
        _ => Err(format!(
            "--topology must be virtual-only, extend or keep, got {text}"
        )),
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
