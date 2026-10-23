//! FFmpeg error text, option dictionaries, and FFmpeg's own log lines routed into a file (a service
//! has no stderr).

use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::ptr;
use std::sync::Mutex;

use ffmpeg_sys_next as ff;

use crate::util::BoxResult;

/// `AVERROR(EAGAIN)`: EAGAIN is 11 in the MinGW CRT the FFmpeg build uses.
pub const EAGAIN: i32 = -11;

static LOG_FILE: Mutex<Option<File>> = Mutex::new(None);

unsafe extern "C" fn log_callback(
    avcl: *mut c_void,
    level: c_int,
    fmt: *const c_char,
    vl: ff::va_list,
) {
    // SAFETY: FFmpeg's own formatter fills our buffer (NUL-terminated, truncated to its size).
    let mut line = [0 as c_char; 1024];
    let mut prefix = 1;
    unsafe {
        if level > ff::av_log_get_level() {
            return;
        }
        ff::av_log_format_line2(
            avcl,
            level,
            fmt,
            vl,
            line.as_mut_ptr(),
            line.len() as c_int,
            &mut prefix,
        );
    }
    // SAFETY: `line` is NUL-terminated by av_log_format_line2.
    let text = unsafe { CStr::from_ptr(line.as_ptr()) }.to_string_lossy();
    if let Ok(mut guard) = LOG_FILE.lock()
        && let Some(file) = guard.as_mut()
    {
        let _ = write!(file, "[{level}] {text}");
    }
}

/// Sends FFmpeg's log lines (up to `level`, e.g. `ff::AV_LOG_VERBOSE`) to `path` from now on.
pub fn log_to_file(path: &Path, level: c_int) -> BoxResult<()> {
    *LOG_FILE.lock().map_err(|_| "log lock poisoned")? = Some(File::create(path)?);
    // SAFETY: the callback is a plain function valid for the whole process.
    unsafe {
        ff::av_log_set_level(level);
        ff::av_log_set_callback(Some(log_callback));
    }
    Ok(())
}

pub fn set_log_level(level: c_int) {
    // SAFETY: plain setter.
    unsafe { ff::av_log_set_level(level) };
}

pub fn err_text(code: i32) -> String {
    let mut buf = [0 as c_char; 128];
    // SAFETY: buffer and its length match; FFmpeg always NUL-terminates.
    unsafe {
        ff::av_strerror(code, buf.as_mut_ptr(), buf.len());
        CStr::from_ptr(buf.as_ptr()).to_string_lossy().into_owned()
    }
}

/// Turns a negative FFmpeg return into an error naming the call.
pub fn check(code: i32, what: &str) -> BoxResult<i32> {
    if code < 0 {
        Err(format!("{what} failed: {} ({code})", err_text(code)).into())
    } else {
        Ok(code)
    }
}

/// Owned `AVDictionary` of codec options; leftovers after `avcodec_open2` are options the encoder lacks.
pub struct Options(pub *mut ff::AVDictionary);

impl Options {
    pub fn new(pairs: &[(&str, String)]) -> BoxResult<Self> {
        let mut dict = ptr::null_mut();
        for (key, value) in pairs {
            let (k, v) = (CString::new(*key)?, CString::new(value.as_str())?);
            // SAFETY: valid C strings; FFmpeg copies them (flags 0).
            check(
                unsafe { ff::av_dict_set(&mut dict, k.as_ptr(), v.as_ptr(), 0) },
                "av_dict_set",
            )?;
        }
        Ok(Self(dict))
    }

    /// Keys still in the dictionary (not consumed by the encoder).
    pub fn leftover_keys(&self) -> Vec<String> {
        let mut keys = Vec::new();
        let mut entry = ptr::null();
        let empty = c"";
        loop {
            // SAFETY: iterating with AV_DICT_IGNORE_SUFFIX and an empty key visits every entry.
            entry = unsafe {
                ff::av_dict_get(self.0, empty.as_ptr(), entry, ff::AV_DICT_IGNORE_SUFFIX)
            };
            if entry.is_null() {
                return keys;
            }
            // SAFETY: entry is a live dictionary entry with a NUL-terminated key.
            keys.push(
                unsafe { CStr::from_ptr((*entry).key) }
                    .to_string_lossy()
                    .into_owned(),
            );
        }
    }
}

impl Drop for Options {
    fn drop(&mut self) {
        // SAFETY: the dictionary is owned here (or null).
        unsafe { ff::av_dict_free(&mut self.0) };
    }
}
