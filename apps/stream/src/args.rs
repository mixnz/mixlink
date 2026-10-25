//! Command line: `--play <file.h264|.hevc> [--fps N] [--loop] [--measure]` (spec 0013 dev mode).
//! No arguments: the window runs as the launcher spawns it.

use std::path::{Path, PathBuf};

use mixlink_codec::VideoCodec;

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
}

/// Parses the arguments after the program name.
pub fn parse(args: impl Iterator<Item = String>) -> Result<Option<PlayArgs>, ArgsError> {
    let mut args = args;
    let (mut path, mut fps, mut looping, mut measure) = (None, None, false, false);
    let mut any = false;
    while let Some(arg) = args.next() {
        any = true;
        match arg.as_str() {
            "--play" => path = Some(args.next().ok_or(ArgsError::MissingValue("--play"))?),
            "--fps" => fps = Some(args.next().ok_or(ArgsError::MissingValue("--fps"))?),
            "--loop" => looping = true,
            "--measure" => measure = true,
            _ => return Err(ArgsError::UnknownFlag(arg)),
        }
    }
    let Some(path) = path.map(PathBuf::from) else {
        return if any {
            Err(ArgsError::NoPlay)
        } else {
            Ok(None)
        };
    };
    let fps = match fps {
        None => FPS_DEFAULT,
        Some(text) => text
            .parse::<u32>()
            .ok()
            .filter(|v| (1..=FPS_MAX).contains(v))
            .ok_or(ArgsError::BadFps(text))?,
    };
    let codec = codec_of(&path).ok_or_else(|| ArgsError::UnknownExtension(path.clone()))?;
    Ok(Some(PlayArgs {
        path,
        codec,
        fps,
        looping,
        measure,
    }))
}

fn codec_of(path: &Path) -> Option<VideoCodec> {
    VideoCodec::from_extension(path.extension()?.to_str()?)
}

#[cfg(test)]
#[path = "args_tests.rs"]
mod tests;
