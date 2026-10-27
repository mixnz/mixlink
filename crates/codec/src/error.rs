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
    #[error("this GPU has no hardware encoder for {0:?}")]
    NoHardwareEncoder(VideoCodec),
    #[error("frame is {got:?}, the encoder was set up for {expected:?}")]
    FrameSize {
        expected: (u32, u32),
        got: (u32, u32),
    },
    #[error("encoded frame of {len} bytes does not fit the access-unit buffer")]
    AuTooLarge { len: usize },
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
