//! Errors of the codec crate.

use crate::VideoCodec;

#[derive(Debug, thiserror::Error)]
pub enum CodecError {
    #[error("{call} failed: {text} ({code})")]
    Ffmpeg {
        call: &'static str,
        code: i32,
        text: String,
    },
    #[error("this GPU has no hardware decoder for {0:?}")]
    NoHardwareDecoder(VideoCodec),
    #[error("corrupt video data")]
    Corrupt,
    #[error("out of memory")]
    OutOfMemory,
    #[cfg(windows)]
    #[error("{context} failed: {source}")]
    Win32 {
        context: &'static str,
        source: windows::core::Error,
    },
}
