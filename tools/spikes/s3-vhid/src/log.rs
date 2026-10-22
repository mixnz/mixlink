//! Run output in `tools/spikes/s3-vhid/out/` (gitignored).

use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::stats::Summary;

pub fn out_dir() -> io::Result<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("out");
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// `<unix seconds>-<name>`: sortable and unique per run.
pub fn run_name(name: &str) -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    format!("{secs}-{name}")
}

pub fn summary_line(s: &Summary) -> String {
    format!(
        "p50 {:.1} us, p99 {:.1} us, max {:.1} us, sent {}, received {}, lost {}",
        s.p50_us, s.p99_us, s.max_us, s.sent, s.received, s.lost
    )
}

/// Writes `<run>.txt` (the given lines) and returns its path.
pub fn write_text(run: &str, lines: &[String]) -> io::Result<PathBuf> {
    let path = out_dir()?.join(format!("{run}.txt"));
    let mut f = fs::File::create(&path)?;
    for line in lines {
        writeln!(f, "{line}")?;
    }
    Ok(path)
}
