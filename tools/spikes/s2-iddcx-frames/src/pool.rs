//! Pooled stats over several 2-minute rounds (spec 0005 → Measurement plan).

use std::fmt::Write as _;

use crate::frames::{FrameTracker, TrackerSummary};
use crate::report::lost_percent;
use crate::stats::percentiles;
use crate::util::BoxResult;

/// `cond` lines that are sampled per round and may differ between rounds of one run.
const PER_ROUND_PREFIX: &str = "cond nvidia-smi";

/// One round: its frames CSV and the `cond ` lines of its summary.
pub struct RoundInput {
    pub name: String,
    pub csv: String,
    pub cond: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoundStats {
    pub file: String,
    pub p99: u64,
    pub lost: u64,
    pub delivered: u64,
    pub undecodable: u64,
}

/// Latencies (µs) of delivered frames and the counts, recomputed with `FrameTracker` rules
/// from a `...-frames.csv` (`t_qpc,counter,ok,generation,latency_us,map_us`).
pub fn parse_frames_csv(text: &str) -> BoxResult<(Vec<u64>, TrackerSummary)> {
    let rows: Vec<&str> = text.lines().skip(1).filter(|l| !l.is_empty()).collect();
    let mut tracker = FrameTracker::new(rows.len());
    for (i, row) in rows.iter().enumerate() {
        let cols: Vec<&str> = row.split(',').collect();
        if cols.len() != 6 {
            return Err(format!("row {}: {} columns, expected 6", i + 2, cols.len()).into());
        }
        let bad = |what: &str| format!("row {}: bad {what}", i + 2);
        let counter = match cols[1] {
            "" => None,
            c => Some(c.parse::<u32>().map_err(|_| bad("counter"))?),
        };
        let ok = cols[2] == "1";
        let latency: u64 = cols[4].parse().map_err(|_| bad("latency_us"))?;
        tracker.on_frame(counter, ok, latency);
    }
    let summary = tracker.summary();
    Ok((tracker.latencies_mut().to_vec(), summary))
}

fn shared_cond(round: &RoundInput) -> Vec<&str> {
    round
        .cond
        .iter()
        .map(String::as_str)
        .filter(|l| !l.starts_with(PER_ROUND_PREFIX))
        .collect()
}

/// Shared `cond` lines, the pooled line, then one line per round. Err when the rounds' conditions
/// differ (other than per-round samples) or there is no round.
pub fn pool(rounds: &[RoundInput]) -> BoxResult<String> {
    let first = rounds.first().ok_or("no rounds to pool")?;
    let cond = shared_cond(first);
    for r in rounds {
        if shared_cond(r) != cond {
            return Err(format!(
                "round {} ran under other conditions than {}",
                r.name, first.name
            )
            .into());
        }
    }

    let mut all = Vec::new();
    let mut total = TrackerSummary::default();
    let mut per_round = Vec::with_capacity(rounds.len());
    for r in rounds {
        let (mut lat, s) = parse_frames_csv(&r.csv).map_err(|e| format!("{}: {e}", r.name))?;
        all.extend_from_slice(&lat);
        total.delivered += s.delivered;
        total.lost += s.lost;
        total.undecodable += s.undecodable;
        per_round.push(RoundStats {
            file: r.name.clone(),
            p99: percentiles(&mut lat).p99,
            lost: s.lost,
            delivered: s.delivered,
            undecodable: s.undecodable,
        });
    }
    let stats = percentiles(&mut all);

    let mut t = String::new();
    for line in cond {
        let _ = writeln!(t, "{line}");
    }
    let _ = writeln!(
        t,
        "pooled {} rounds: p50 {} us, p99 {} us, max {} us ({} samples); lost {} / {} ({:.3} %), undecodable {}",
        rounds.len(),
        stats.p50,
        stats.p99,
        stats.max,
        stats.count,
        total.lost,
        total.lost + total.delivered,
        lost_percent(total.lost, total.delivered),
        total.undecodable
    );
    for r in &per_round {
        let _ = writeln!(
            t,
            "round {}: p99 {} us, lost {} / {} ({:.3} %), undecodable {}",
            r.file,
            r.p99,
            r.lost,
            r.lost + r.delivered,
            lost_percent(r.lost, r.delivered),
            r.undecodable
        );
    }
    Ok(t)
}

#[cfg(test)]
#[path = "pool_tests.rs"]
mod tests;
