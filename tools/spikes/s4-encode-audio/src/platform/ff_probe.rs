//! Which hardware encoders the loaded FFmpeg build has, and its version.

use std::ffi::{CStr, CString};

use ffmpeg_sys_next as ff;

const ENCODERS: [&str; 7] = [
    "h264_nvenc",
    "hevc_nvenc",
    "av1_nvenc",
    "h264_qsv",
    "hevc_qsv",
    "h264_amf",
    "hevc_amf",
];

/// (encoder name, found in this build).
pub fn encoders() -> Vec<(String, bool)> {
    ENCODERS
        .iter()
        .map(|name| {
            let c = CString::new(*name).unwrap_or_default();
            // SAFETY: `c` is a valid NUL-terminated string; the result is only null-checked.
            let found = unsafe { !ff::avcodec_find_encoder_by_name(c.as_ptr()).is_null() };
            (name.to_string(), found)
        })
        .collect()
}

pub fn version() -> String {
    // SAFETY: FFmpeg returns a static NUL-terminated string.
    let info = unsafe { CStr::from_ptr(ff::av_version_info()) };
    // SAFETY: plain getter.
    let lavc = unsafe { ff::avcodec_version() };
    format!(
        "FFmpeg {} (libavcodec {}.{}.{})",
        info.to_string_lossy(),
        lavc >> 16,
        lavc >> 8 & 0xFF,
        lavc & 0xFF
    )
}
