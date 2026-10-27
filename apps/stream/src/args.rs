//! Command line: `--connect <host id> --addr <ip:port> [--drop video=N,cursor=N]` (spec 0020, the launcher
//! spawns this) or `--play <file.h264|.hevc> [--fps N] [--loop] [--measure]` (spec 0013 dev mode).
//! No arguments: an empty window (launcher demo).

use std::net::SocketAddrV4;
use std::path::{Path, PathBuf};

use mixlink_codec::VideoCodec;
use mixlink_core::ids::DeviceId;

use crate::net::drop_knob::DropKnob;

const FPS_DEFAULT: u32 = 120;
const FPS_MAX: u32 = 240;

/// Play a recorded Annex-B stream instead of the network.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayArgs {
    pub path: PathBuf,
    pub codec: VideoCodec,
    pub fps: u32,
    pub looping: bool,
    /// Wait for the GPU after each decode so `DecodeEnd` is GPU-complete (measuring only).
    pub measure: bool,
}

/// Stream from a paired host (spec 0020).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectArgs {
    pub host: DeviceId,
    pub addr: SocketAddrV4,
    pub drop: DropKnob,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    Play(PlayArgs),
    Connect(ConnectArgs),
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ArgsError {
    #[error("unknown option {0}")]
    UnknownFlag(String),
    #[error("{0} needs a value")]
    MissingValue(&'static str),
    #[error("--fps must be 1..={FPS_MAX}, got {0}")]
    BadFps(String),
    #[error("{0}: not a .h264 / .hevc file")]
    UnknownExtension(PathBuf),
    #[error("--fps, --loop and --measure need --play")]
    NoPlay,
    #[error("--connect needs --addr <ip:port>")]
    NoAddr,
    #[error("{0}: not a host id (32 hex digits)")]
    BadHost(String),
    #[error("{0}: not an IPv4 address with a port")]
    BadAddr(String),
    #[error("--drop {0}: expected video=N,cursor=N with N in 0..=100")]
    BadDrop(String),
    #[error("--connect and --play cannot be used together")]
    BothModes,
    #[error("--addr and --drop need --connect")]
    NoConnect,
}

/// Every option as given, before the mode is known.
#[derive(Debug, Default)]
struct Raw {
    play: Option<String>,
    fps: Option<String>,
    looping: bool,
    measure: bool,
    connect: Option<String>,
    addr: Option<String>,
    drop: Option<String>,
}

/// Parses the arguments after the program name. `None`: no arguments.
pub fn parse(args: impl Iterator<Item = String>) -> Result<Option<Mode>, ArgsError> {
    let mut args = args;
    let mut raw = Raw::default();
    let mut any = false;
    while let Some(arg) = args.next() {
        any = true;
        let mut value = |name: &'static str| args.next().ok_or(ArgsError::MissingValue(name));
        match arg.as_str() {
            "--play" => raw.play = Some(value("--play")?),
            "--fps" => raw.fps = Some(value("--fps")?),
            "--loop" => raw.looping = true,
            "--measure" => raw.measure = true,
            "--connect" => raw.connect = Some(value("--connect")?),
            "--addr" => raw.addr = Some(value("--addr")?),
            "--drop" => raw.drop = Some(value("--drop")?),
            _ => return Err(ArgsError::UnknownFlag(arg)),
        }
    }
    if !any {
        return Ok(None);
    }
    match (raw.connect.is_some(), raw.play.is_some()) {
        (true, true) => Err(ArgsError::BothModes),
        (true, false) => connect(raw).map(|c| Some(Mode::Connect(c))),
        (false, _) => play(raw).map(|p| Some(Mode::Play(p))),
    }
}

fn connect(raw: Raw) -> Result<ConnectArgs, ArgsError> {
    if raw.fps.is_some() || raw.looping || raw.measure {
        return Err(ArgsError::NoPlay);
    }
    let host_text = raw.connect.unwrap_or_default();
    let host = host_text
        .parse::<DeviceId>()
        .map_err(|_| ArgsError::BadHost(host_text.clone()))?;
    let addr_text = raw.addr.ok_or(ArgsError::NoAddr)?;
    let addr = addr_text
        .parse::<SocketAddrV4>()
        .map_err(|_| ArgsError::BadAddr(addr_text.clone()))?;
    let drop = match raw.drop {
        Some(text) => DropKnob::parse(&text)?,
        None => DropKnob::default(),
    };
    Ok(ConnectArgs { host, addr, drop })
}

fn play(raw: Raw) -> Result<PlayArgs, ArgsError> {
    if raw.addr.is_some() || raw.drop.is_some() {
        return Err(ArgsError::NoConnect);
    }
    let path = PathBuf::from(raw.play.ok_or(ArgsError::NoPlay)?);
    let fps = match raw.fps {
        None => FPS_DEFAULT,
        Some(text) => text
            .parse::<u32>()
            .ok()
            .filter(|v| (1..=FPS_MAX).contains(v))
            .ok_or(ArgsError::BadFps(text))?,
    };
    let codec = codec_of(&path).ok_or_else(|| ArgsError::UnknownExtension(path.clone()))?;
    Ok(PlayArgs {
        path,
        codec,
        fps,
        looping: raw.looping,
        measure: raw.measure,
    })
}

fn codec_of(path: &Path) -> Option<VideoCodec> {
    VideoCodec::from_extension(path.extension()?.to_str()?)
}

#[cfg(test)]
#[path = "args_tests.rs"]
mod tests;
