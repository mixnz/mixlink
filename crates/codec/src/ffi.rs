//! Small helpers around the FFmpeg C API (`ffmpeg-sys-next`).

use std::ffi::{CStr, c_char};

use ffmpeg_sys_next as ff;

use crate::{CodecError, VideoCodec};

/// Maps a negative FFmpeg return code to `CodecError::Ffmpeg`; passes others through.
pub(crate) fn check(code: i32, call: &'static str) -> Result<i32, CodecError> {
    if code >= 0 {
        return Ok(code);
    }
    Err(CodecError::Ffmpeg {
        call,
        code,
        text: error_text(code),
    })
}

fn error_text(code: i32) -> String {
    let mut buf = [0 as c_char; 128];
    // SAFETY: `buf` is a writable buffer of the given size; av_strerror NUL-terminates it.
    let text = unsafe {
        ff::av_strerror(code, buf.as_mut_ptr(), buf.len());
        CStr::from_ptr(buf.as_ptr())
    };
    text.to_string_lossy().into_owned()
}

pub(crate) fn codec_id(codec: VideoCodec) -> ff::AVCodecID {
    match codec {
        VideoCodec::H264 => ff::AVCodecID::AV_CODEC_ID_H264,
        VideoCodec::Hevc => ff::AVCodecID::AV_CODEC_ID_HEVC,
    }
}

/// `AVERROR(EAGAIN)`: EAGAIN is 11 in the MinGW CRT the FFmpeg build uses.
pub(crate) const EAGAIN: i32 = -11;
