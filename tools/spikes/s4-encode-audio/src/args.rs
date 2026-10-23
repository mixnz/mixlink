//! Job arguments shared by `s4-svc` (service, `--console`, `--in-console`) and the scripts.

use crate::util::{BoxResult, arg_u64, arg_value};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    Nvenc,
    Qsv,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Codec {
    H264,
    Hevc,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Bgra,
    Nv12,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EncodeArgs {
    pub backend: Backend,
    pub codec: Codec,
    pub format: Format,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub frames: u32,
    pub bitrate: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AudioArgs {
    pub minutes: u32,
    pub tone: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Job {
    Encode(EncodeArgs),
    Audio(AudioArgs),
    Both(EncodeArgs, AudioArgs),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunArgs {
    pub job: Job,
    pub case: String,
    pub side: char,
    pub delay_s: u32,
    /// Run the job in this process (user session or console session), not as a service.
    pub console: bool,
    /// Service only: spawn self as SYSTEM in the console session (side C).
    pub in_console: bool,
}

impl Codec {
    pub fn ext(self) -> &'static str {
        match self {
            Self::H264 => "h264",
            Self::Hevc => "hevc",
        }
    }
}

impl EncodeArgs {
    /// FFmpeg encoder name, e.g. `h264_nvenc`.
    pub fn encoder_name(&self) -> String {
        let backend = match self.backend {
            Backend::Nvenc => "nvenc",
            Backend::Qsv => "qsv",
        };
        format!("{}_{backend}", self.codec.ext())
    }
}

fn u32_arg(args: &[String], name: &str, default: u32) -> BoxResult<u32> {
    Ok(u32::try_from(arg_u64(args, name, u64::from(default))?)?)
}

fn parse_size(text: &str) -> BoxResult<(u32, u32)> {
    let (w, h) = text
        .split_once('x')
        .ok_or("--size must look like 2560x1440")?;
    Ok((w.parse()?, h.parse()?))
}

fn parse_encode(args: &[String]) -> BoxResult<EncodeArgs> {
    let backend = match arg_value(args, "--backend").unwrap_or("nvenc") {
        "nvenc" => Backend::Nvenc,
        "qsv" => Backend::Qsv,
        other => return Err(format!("unknown backend {other}").into()),
    };
    let codec = match arg_value(args, "--codec").unwrap_or("h264") {
        "h264" => Codec::H264,
        "hevc" => Codec::Hevc,
        other => return Err(format!("unknown codec {other}").into()),
    };
    let format = match arg_value(args, "--format") {
        Some("bgra") => Format::Bgra,
        Some("nv12") => Format::Nv12,
        Some(other) => return Err(format!("unknown format {other}").into()),
        None if backend == Backend::Qsv => Format::Nv12,
        None => Format::Bgra,
    };
    let (width, height) = parse_size(arg_value(args, "--size").unwrap_or("2560x1440"))?;
    Ok(EncodeArgs {
        backend,
        codec,
        format,
        width,
        height,
        fps: u32_arg(args, "--fps", 120)?,
        frames: u32_arg(args, "--frames", 10_000)?,
        bitrate: arg_u64(args, "--bitrate", 30_000_000)?,
    })
}

fn parse_audio(args: &[String]) -> BoxResult<AudioArgs> {
    Ok(AudioArgs {
        minutes: u32_arg(args, "--minutes", 10)?,
        tone: args.iter().any(|a| a == "--tone"),
    })
}

/// Parses `[--console] [--in-console] --case X --side A <encode|audio|both> [options]` (program name excluded).
pub fn parse(args: &[String]) -> BoxResult<RunArgs> {
    let job_word = args
        .iter()
        .find(|a| matches!(a.as_str(), "encode" | "audio" | "both"))
        .ok_or("missing job: encode, audio or both")?;
    let job = match job_word.as_str() {
        "encode" => Job::Encode(parse_encode(args)?),
        "audio" => Job::Audio(parse_audio(args)?),
        _ => Job::Both(parse_encode(args)?, parse_audio(args)?),
    };
    let side = arg_value(args, "--side").unwrap_or("A");
    let side = match side {
        "A" | "B" | "C" => side.chars().next().unwrap_or('A'),
        other => return Err(format!("unknown side {other}").into()),
    };
    Ok(RunArgs {
        job,
        case: arg_value(args, "--case").unwrap_or("dev").to_string(),
        side,
        delay_s: u32_arg(args, "--delay-seconds", 0)?,
        console: args.iter().any(|a| a == "--console"),
        in_console: args.iter().any(|a| a == "--in-console"),
    })
}

#[cfg(test)]
#[path = "args_tests.rs"]
mod tests;
