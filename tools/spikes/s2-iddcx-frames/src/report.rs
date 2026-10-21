//! Run summary (`<stamp>-<case>-<kind>-summary.txt`) and per-frame latencies (`...-frames.csv`).

use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;

use crate::framelog::{CSV_HEADER, FrameLog, csv_line};
use crate::frames::{FrameTracker, TrackerSummary};
use crate::qpc;
use crate::stats::{Stats, percentiles};
use crate::util::{BoxResult, now_stamp, out_dir};

pub fn lost_percent(lost: u64, delivered: u64) -> f64 {
    let total = lost + delivered;
    if total == 0 {
        return 0.0;
    }
    lost as f64 * 100.0 / total as f64
}

/// `ref_counts` = [frames measured from present time, frames measured from acquire time].
pub fn summary_text(
    case: &str,
    kind: &str,
    s: &TrackerSummary,
    stats: &Stats,
    ref_counts: [u64; 2],
    notes: &[String],
) -> String {
    let mut t = String::new();
    let _ = writeln!(t, "case {case} ({kind})");
    let _ = writeln!(
        t,
        "latency p50 {} us, p99 {} us, max {} us ({} samples)",
        stats.p50, stats.p99, stats.max, stats.count
    );
    let _ = writeln!(
        t,
        "frames delivered {}, lost {} / {} ({:.3} %), duplicates {}, undecodable {}, overwritten {}, overflow {}",
        s.delivered,
        s.lost,
        s.lost + s.delivered,
        lost_percent(s.lost, s.delivered),
        s.duplicates,
        s.undecodable,
        s.overwritten,
        s.overflow
    );
    let _ = writeln!(
        t,
        "reference present {}, acquire {}",
        ref_counts[0], ref_counts[1]
    );
    for note in notes {
        let _ = writeln!(t, "{note}");
    }
    t
}

/// Converts the tracker's latencies (QPC ticks) to µs in place, then writes the summary and the
/// per-frame CSV from `log`. Harness cost (counter-cell Map) is added to the notes.
pub fn write_run(
    case: &str,
    kind: &str,
    tracker: &mut FrameTracker,
    log: &FrameLog,
    ref_counts: [u64; 2],
    notes: &[String],
) -> BoxResult<PathBuf> {
    let summary = tracker.summary();
    let latencies = tracker.latencies_mut();
    for v in latencies.iter_mut() {
        *v = qpc::to_us(*v);
    }
    let stats = percentiles(latencies);

    let freq = qpc::freq();
    let mut csv = String::from(CSV_HEADER);
    csv.push('\n');
    let mut map_us = Vec::with_capacity(log.records().len());
    for r in log.records() {
        let _ = writeln!(csv, "{}", csv_line(r, freq));
        map_us.push(qpc::to_us(r.map_qpc));
    }
    let map = percentiles(&mut map_us);
    let mut notes = notes.to_vec();
    notes.push(format!(
        "harness counter-cell map p50 {} us, p99 {} us, max {} us; frame log overflow {}",
        map.p50,
        map.p99,
        map.max,
        log.overflow()
    ));

    let stem = format!("{}-{case}-{kind}", now_stamp());
    let dir = out_dir()?;
    fs::write(dir.join(format!("{stem}-frames.csv")), csv)?;
    let path = dir.join(format!("{stem}-summary.txt"));
    fs::write(
        &path,
        summary_text(case, kind, &summary, &stats, ref_counts, &notes),
    )?;
    Ok(path)
}

#[cfg(test)]
#[path = "report_tests.rs"]
mod tests;
