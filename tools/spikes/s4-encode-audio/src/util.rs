//! Small helpers shared by the spike binaries: args, output folder, timestamps.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

pub type BoxResult<T> = Result<T, Box<dyn std::error::Error>>;

/// Value after `name` (`--case A` → `Some("A")`).
pub fn arg_value<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    let i = args.iter().position(|a| a == name)?;
    args.get(i + 1).map(String::as_str)
}

pub fn arg_u64(args: &[String], name: &str, default: u64) -> BoxResult<u64> {
    match arg_value(args, name) {
        Some(v) => Ok(v.parse()?),
        None => Ok(default),
    }
}

/// `tools/spikes/s4-encode-audio/out/` (absolute, baked in at build time; the service runs from anywhere).
pub fn out_dir() -> BoxResult<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("out");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// `yyyyMMdd-HHmmss`, UTC.
pub fn utc_stamp(unix_secs: u64) -> String {
    let days = (unix_secs / 86_400) as i64;
    let rem = unix_secs % 86_400;
    // Civil-from-days (Howard Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}{month:02}{day:02}-{:02}{:02}{:02}",
        rem / 3_600,
        rem / 60 % 60,
        rem % 60
    )
}

pub fn now_stamp() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    utc_stamp(secs)
}

#[cfg(test)]
#[path = "util_tests.rs"]
mod tests;
