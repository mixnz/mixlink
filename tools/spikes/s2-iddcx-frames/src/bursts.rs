//! Loss bursts (spec 0005): where lost or undecodable frames cluster in a run, so a round with
//! high loss can be explained. Computed after the run, never in the hot path.

use crate::framelog::FrameRecord;

/// Largest bursts listed in a summary.
const NOTE_LIMIT: usize = 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Burst {
    /// QPC of the first record of the burst.
    pub start_qpc: u64,
    /// Counter values skipped inside the burst.
    pub lost: u32,
    /// Records without a counter (undecodable or overwritten).
    pub undecodable: u32,
}

/// A burst = consecutive records with a counter gap or no counter, closed by the next good record.
/// Duplicates (same counter again) are not loss.
pub fn bursts(records: &[FrameRecord]) -> Vec<Burst> {
    let mut out = Vec::new();
    let mut open: Option<Burst> = None;
    let mut last: Option<u32> = None;
    for r in records {
        let Some(n) = r.counter else {
            open.get_or_insert(Burst {
                start_qpc: r.t_qpc,
                lost: 0,
                undecodable: 0,
            })
            .undecodable += 1;
            continue;
        };
        let gap = match last {
            Some(prev) if n > prev => n - prev - 1,
            _ => 0,
        };
        last = Some(n);
        if gap > 0 {
            open.get_or_insert(Burst {
                start_qpc: r.t_qpc,
                lost: 0,
                undecodable: 0,
            })
            .lost += gap;
        }
        if let Some(b) = open.take() {
            out.push(b);
        }
    }
    out.extend(open);
    out
}

/// Summary lines: totals, then the largest bursts (lost + undecodable) with ms since `run_start_qpc`.
pub fn burst_notes(bursts: &[Burst], run_start_qpc: u64, qpc_freq: u64) -> Vec<String> {
    let lost: u64 = bursts.iter().map(|b| u64::from(b.lost)).sum();
    let undecodable: u64 = bursts.iter().map(|b| u64::from(b.undecodable)).sum();
    let mut notes = vec![format!(
        "loss bursts {}, lost {lost}, undecodable {undecodable}",
        bursts.len()
    )];
    let mut largest: Vec<&Burst> = bursts.iter().collect();
    largest.sort_by_key(|b| std::cmp::Reverse(b.lost + b.undecodable));
    for b in largest.into_iter().take(NOTE_LIMIT) {
        let ms = b.start_qpc.saturating_sub(run_start_qpc) * 1_000 / qpc_freq.max(1);
        notes.push(format!(
            "burst at {ms} ms: lost {}, undecodable {}",
            b.lost, b.undecodable
        ));
    }
    notes
}

#[cfg(test)]
#[path = "bursts_tests.rs"]
mod tests;
