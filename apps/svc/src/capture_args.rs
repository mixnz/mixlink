//! Command line of `mixlink-svc capture ...` (spec 0019): subcommand + `--flag value` pairs in any order.

use std::path::{Path, PathBuf};

use mixlink_codec::VideoCodec;
use mixlink_core::ids::DeviceId;
use mixlink_proto::control::DisplayInfo;

use crate::args::{TryTopology, parse_mode, parse_topology};

/// `capture ...` subcommands: developer checks of capture + encode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureCmd {
    Try(CaptureTry),
}

/// Adds a monitor for `client` at `mode`, records `seconds` of it to `out`, removes the monitor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureTry {
    pub client: DeviceId,
    pub mode: DisplayInfo,
    pub topology: TryTopology,
    pub codec: VideoCodec,
    pub seconds: u32,
    pub bitrate_kbps: u32,
    pub out: PathBuf,
    pub levers: Levers,
}

/// Priority levers of the capture thread (spec 0007 Verdict), each switchable to measure it alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Levers {
    /// MMCSS "Pro Audio" at critical priority.
    pub mmcss: bool,
    /// `SetGPUThreadPriority(7)` on the capture device.
    pub gpu: bool,
    /// `HIGH_PRIORITY_CLASS` for the process.
    pub class: bool,
}

impl Levers {
    pub const ALL: Levers = Levers {
        mmcss: true,
        gpu: true,
        class: true,
    };
}

const DEFAULT_SECONDS: u32 = 10;
const MAX_SECONDS: u32 = 600;
const DEFAULT_BITRATE_KBPS: u32 = 30_000;
const BITRATE_RANGE_KBPS: std::ops::RangeInclusive<u32> = 1_000..=500_000;

#[derive(Default)]
struct Flags<'a> {
    client: Option<&'a str>,
    mode: Option<&'a str>,
    topology: Option<&'a str>,
    codec: Option<&'a str>,
    seconds: Option<&'a str>,
    bitrate: Option<&'a str>,
    out: Option<&'a str>,
    levers: Option<&'a str>,
}

/// `args` = everything after `capture`.
pub fn parse_capture(args: &[&str]) -> Result<CaptureCmd, String> {
    let (command, rest) = args.split_first().ok_or("capture needs a subcommand")?;
    if *command != "try" {
        return Err(format!("unknown capture subcommand: {command}"));
    }
    let flags = parse_flags(rest)?;
    let client = flags.client.ok_or("capture try needs --client")?;
    let mode = flags.mode.ok_or("capture try needs --mode")?;
    let out = PathBuf::from(flags.out.ok_or("capture try needs --out")?);
    let codec = flags.codec.map_or(Ok(VideoCodec::H264), parse_codec)?;
    check_extension(&out, codec)?;
    Ok(CaptureCmd::Try(CaptureTry {
        client: client
            .parse()
            .map_err(|_| format!("--client needs 32 hex digits, got {client}"))?,
        mode: parse_mode(mode)?,
        topology: flags
            .topology
            .map_or(Ok(TryTopology::Keep), parse_topology)?,
        codec,
        seconds: flags.seconds.map_or(Ok(DEFAULT_SECONDS), parse_seconds)?,
        bitrate_kbps: flags
            .bitrate
            .map_or(Ok(DEFAULT_BITRATE_KBPS), parse_bitrate)?,
        out,
        levers: flags.levers.map_or(Ok(Levers::ALL), parse_levers)?,
    }))
}

fn parse_flags<'a>(rest: &[&'a str]) -> Result<Flags<'a>, String> {
    let mut flags = Flags::default();
    for pair in rest.chunks(2) {
        let [name, value] = pair else {
            return Err(format!("{} needs a value", pair[0]));
        };
        let field = match *name {
            "--client" => &mut flags.client,
            "--mode" => &mut flags.mode,
            "--topology" => &mut flags.topology,
            "--codec" => &mut flags.codec,
            "--seconds" => &mut flags.seconds,
            "--bitrate" => &mut flags.bitrate,
            "--out" => &mut flags.out,
            "--levers" => &mut flags.levers,
            _ => return Err(format!("unknown flag {name}")),
        };
        if field.replace(value).is_some() {
            return Err(format!("{name} given twice"));
        }
    }
    Ok(flags)
}

fn parse_codec(text: &str) -> Result<VideoCodec, String> {
    match text {
        "h264" => Ok(VideoCodec::H264),
        "hevc" => Ok(VideoCodec::Hevc),
        _ => Err(format!("--codec must be h264 or hevc, got {text}")),
    }
}

/// `mixlink-stream --play` picks the decoder from the extension, so it must match the codec.
fn check_extension(out: &Path, codec: VideoCodec) -> Result<(), String> {
    let ext = out.extension().and_then(|e| e.to_str()).unwrap_or("");
    if VideoCodec::from_extension(ext) == Some(codec) {
        return Ok(());
    }
    let want = match codec {
        VideoCodec::H264 => ".h264",
        VideoCodec::Hevc => ".hevc",
    };
    Err(format!(
        "--out must end in {want} for this codec, got {}",
        out.display()
    ))
}

fn parse_seconds(text: &str) -> Result<u32, String> {
    text.parse::<u32>()
        .ok()
        .filter(|s| (1..=MAX_SECONDS).contains(s))
        .ok_or_else(|| format!("--seconds must be 1..={MAX_SECONDS}, got {text}"))
}

fn parse_bitrate(text: &str) -> Result<u32, String> {
    text.parse::<u32>()
        .ok()
        .filter(|k| BITRATE_RANGE_KBPS.contains(k))
        .ok_or_else(|| {
            format!(
                "--bitrate must be {}..={} kbps, got {text}",
                BITRATE_RANGE_KBPS.start(),
                BITRATE_RANGE_KBPS.end()
            )
        })
}

/// `none`, or a comma list of `mmcss`, `gpu`, `class`.
fn parse_levers(text: &str) -> Result<Levers, String> {
    let mut levers = Levers {
        mmcss: false,
        gpu: false,
        class: false,
    };
    if text == "none" {
        return Ok(levers);
    }
    for name in text.split(',') {
        let field = match name {
            "mmcss" => &mut levers.mmcss,
            "gpu" => &mut levers.gpu,
            "class" => &mut levers.class,
            _ => {
                return Err(format!(
                    "--levers must be none or a list of mmcss, gpu, class, got {text}"
                ));
            }
        };
        *field = true;
    }
    Ok(levers)
}

#[cfg(test)]
#[path = "capture_args_tests.rs"]
mod tests;
