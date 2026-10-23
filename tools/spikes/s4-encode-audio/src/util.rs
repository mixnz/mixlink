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

/// `tools/spikes/s4-encode-audio/out/` (baked in at build time) while the exe runs from `target\`;
/// `<exe folder>\out\` for a packed copy (laptop runs). The service runs from anywhere either way.
pub fn out_dir() -> BoxResult<PathBuf> {
    let exe = std::env::current_exe()?;
    let in_target = exe.components().any(|c| c.as_os_str() == "target");
    let dir = match exe.parent() {
        Some(folder) if !in_target => folder.join("out"),
        _ => PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("out"),
    };
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

/// Output files of one run: `out/<stamp>-<case>-<side>-<suffix>`.
pub struct RunFiles {
    pub stamp: String,
    pub case: String,
    pub side: char,
}

impl RunFiles {
    pub fn new(case: &str, side: char) -> Self {
        Self {
            stamp: now_stamp(),
            case: case.to_string(),
            side,
        }
    }

    pub fn path(&self, suffix: &str) -> PathBuf {
        let dir = out_dir().unwrap_or_else(|_| PathBuf::from("."));
        dir.join(format!(
            "{}-{}-{}-{suffix}",
            self.stamp, self.case, self.side
        ))
    }

    /// Summary text file plus one µs sample per line in a CSV.
    pub fn write_result(&self, job: &str, summary: &str, samples_us: &[u64]) -> BoxResult<()> {
        std::fs::write(self.path(&format!("{job}.txt")), summary)?;
        let mut csv = String::with_capacity(samples_us.len() * 6);
        csv.push_str("# us\n");
        for v in samples_us {
            csv.push_str(&v.to_string());
            csv.push('\n');
        }
        std::fs::write(self.path(&format!("{job}.csv")), csv)?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "util_tests.rs"]
mod tests;
