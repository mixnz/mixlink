//! Command line of `mixlink-svc input ...` (spec 0016): subcommand + `--flag value` pairs in any order.

/// `input ...` subcommands: developer checks of the virtual input driver.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputCmd {
    Status,
    /// Presses pad `slot`'s controls typed on stdin (`a`, `lt 2000`, ...) for mapping wizards and
    /// games (`input::padmap`); each press waits `delay_s` first (time to switch to the game).
    Pad {
        delay_s: u32,
        slot: u8,
    },
    /// Plays `script` after `delay_s` seconds (time to focus a game, UAC prompt or lock screen);
    /// pad steps go to `slot`.
    Try {
        script: Script,
        delay_s: u32,
        slot: u8,
    },
    /// Hidden: latency of `count` events through the driver (`input::probe`); pad on `slot`.
    Probe {
        device: ProbeDevice,
        count: u32,
        slot: u8,
    },
    /// Creates the virtual Xbox pad node of `slot` (`input::pads`).
    PadAdd {
        slot: u8,
    },
    /// Removes the virtual Xbox pad node of `slot`.
    PadRemove {
        slot: u8,
    },
}

/// Virtual Xbox pads: slots 0..PAD_SLOTS (split screen).
pub const PAD_SLOTS: u8 = mixlink_input::hid::xbox::PAD_SLOTS;

/// Which `vhid` collection `input probe` measures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeDevice {
    Kbd,
    Rel,
    Abs,
    Pad,
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
    /// Mouse square, then Tab ×3 (UAC prompt check: moves focus, never confirms).
    Uac,
    /// Mouse square, then Space (lock screen check: shows the sign-in box).
    Lock,
    /// W held 1 s, look right then back (game check).
    Walk,
}

/// `input probe` default and largest event count.
const PROBE_COUNT: u32 = 10_000;
const MAX_PROBE_COUNT: u32 = 100_000;
/// Longest `--delay`.
const MAX_DELAY_S: u32 = 600;

/// `--flag value` pairs after the subcommand; each flag at most once.
#[derive(Default)]
struct Flags<'a> {
    script: Option<&'a str>,
    delay: Option<&'a str>,
    slot: Option<&'a str>,
    device: Option<&'a str>,
    count: Option<&'a str>,
}

/// `args` = everything after `input`.
pub fn parse_input(args: &[&str]) -> Result<InputCmd, String> {
    let (command, rest) = args.split_first().ok_or("input needs a subcommand")?;
    let flags = parse_flags(rest)?;
    let allow = |names: &[&str]| -> Result<(), String> {
        let given = [
            ("--script", flags.script.is_some()),
            ("--delay", flags.delay.is_some()),
            ("--slot", flags.slot.is_some()),
            ("--device", flags.device.is_some()),
            ("--count", flags.count.is_some()),
        ];
        match given
            .iter()
            .find(|(name, set)| *set && !names.contains(name))
        {
            Some((name, _)) => Err(format!("input {command} doesn't take {name}")),
            None => Ok(()),
        }
    };
    let slot = flags.slot.map_or(Ok(0), parse_slot)?;
    let delay_s = flags.delay.map_or(Ok(0), parse_delay)?;
    match *command {
        "status" => allow(&[]).map(|()| InputCmd::Status),
        "pad" => allow(&["--delay", "--slot"]).map(|()| InputCmd::Pad { delay_s, slot }),
        "try" => {
            allow(&["--script", "--delay", "--slot"])?;
            let script = flags.script.ok_or("input try needs --script")?;
            Ok(InputCmd::Try {
                script: parse_script(script)?,
                delay_s,
                slot,
            })
        }
        "probe" => {
            allow(&["--device", "--count", "--slot"])?;
            let device = flags.device.ok_or("input probe needs --device")?;
            Ok(InputCmd::Probe {
                device: parse_probe_device(device)?,
                count: flags.count.map_or(Ok(PROBE_COUNT), parse_count)?,
                slot,
            })
        }
        "pad-add" | "pad-remove" => {
            allow(&["--slot"])?;
            flags.slot.ok_or(format!("input {command} needs --slot"))?;
            Ok(if *command == "pad-add" {
                InputCmd::PadAdd { slot }
            } else {
                InputCmd::PadRemove { slot }
            })
        }
        _ => Err(format!("unknown input subcommand: {command}")),
    }
}

fn parse_flags<'a>(rest: &[&'a str]) -> Result<Flags<'a>, String> {
    let mut flags = Flags::default();
    let mut pairs = rest.chunks(2);
    for pair in &mut pairs {
        let [name, value] = pair else {
            return Err(format!("{} needs a value", pair[0]));
        };
        let field = match *name {
            "--script" => &mut flags.script,
            "--delay" => &mut flags.delay,
            "--slot" => &mut flags.slot,
            "--device" => &mut flags.device,
            "--count" => &mut flags.count,
            _ => return Err(format!("unknown flag {name}")),
        };
        if field.replace(value).is_some() {
            return Err(format!("{name} given twice"));
        }
    }
    Ok(flags)
}

fn parse_slot(text: &str) -> Result<u8, String> {
    text.parse::<u8>()
        .ok()
        .filter(|&s| s < PAD_SLOTS)
        .ok_or_else(|| format!("--slot must be 0..={}, got {text}", PAD_SLOTS - 1))
}

fn parse_delay(text: &str) -> Result<u32, String> {
    text.parse::<u32>()
        .ok()
        .filter(|&s| s <= MAX_DELAY_S)
        .ok_or_else(|| format!("--delay must be 0..={MAX_DELAY_S} seconds, got {text}"))
}

fn parse_count(text: &str) -> Result<u32, String> {
    text.parse::<u32>()
        .ok()
        .filter(|n| (1..=MAX_PROBE_COUNT).contains(n))
        .ok_or_else(|| format!("--count must be 1..={MAX_PROBE_COUNT}, got {text}"))
}

fn parse_probe_device(text: &str) -> Result<ProbeDevice, String> {
    match text {
        "kbd" => Ok(ProbeDevice::Kbd),
        "rel" => Ok(ProbeDevice::Rel),
        "abs" => Ok(ProbeDevice::Abs),
        "pad" => Ok(ProbeDevice::Pad),
        _ => Err(format!("--device must be kbd, rel, abs or pad, got {text}")),
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
        "uac" => Ok(Script::Uac),
        "lock" => Ok(Script::Lock),
        "walk" => Ok(Script::Walk),
        _ => Err(format!(
            "--script must be keys, chord, mouse, abs, pad, all, hold, uac, lock or walk, got {text}"
        )),
    }
}
