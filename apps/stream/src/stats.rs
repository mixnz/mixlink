//! Per-frame stage timings of the last N frames, summarized when the window closes.

use std::fmt::Write as _;
use std::time::Duration;

use mixlink_core::ids::FrameId;
use mixlink_core::stage::{Stage, StageStamps};

/// Frames kept for the summary.
pub const STATS_FRAMES: usize = 10_000;

/// Spans printed in the summary: (name, from, to).
const SPANS: [(&str, Stage, Stage); 4] = [
    ("queue", Stage::Received, Stage::DecodeStart),
    ("decode", Stage::DecodeStart, Stage::DecodeEnd),
    ("present", Stage::DecodeEnd, Stage::Presented),
    ("total", Stage::Received, Stage::Presented),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpanStats {
    pub count: usize,
    pub p50: Duration,
    pub p99: Duration,
    pub max: Duration,
}

/// Ring of the last `capacity` frames plus counters; `record` never allocates.
#[derive(Debug)]
pub struct FrameStats {
    ring: Box<[StageStamps]>,
    next: usize,
    filled: usize,
    pub decoded: u64,
    pub presented: u64,
    /// Decoded but replaced by a newer frame before it was presented.
    pub dropped: u64,
    pub corrupt: u64,
}

impl FrameStats {
    pub fn new(capacity: usize) -> Self {
        let empty = StageStamps::new(FrameId::new(0));
        Self {
            ring: vec![empty; capacity.max(1)].into_boxed_slice(),
            next: 0,
            filled: 0,
            decoded: 0,
            presented: 0,
            dropped: 0,
            corrupt: 0,
        }
    }

    // HOT PATH
    pub fn record(&mut self, stamps: &StageStamps) {
        self.ring[self.next] = *stamps;
        self.next = (self.next + 1) % self.ring.len();
        self.filled = (self.filled + 1).min(self.ring.len());
    }

    /// Nearest-rank percentiles of `from` → `to` over frames that reached both.
    pub fn span(&self, from: Stage, to: Stage) -> Option<SpanStats> {
        let mut spans: Vec<Duration> = self.ring[..self.filled]
            .iter()
            .filter_map(|s| s.span(from, to))
            .collect();
        if spans.is_empty() {
            return None;
        }
        spans.sort_unstable();
        let rank = |p: usize| spans[(spans.len() * p).div_ceil(100).max(1) - 1];
        Some(SpanStats {
            count: spans.len(),
            p50: rank(50),
            p99: rank(99),
            max: spans[spans.len() - 1],
        })
    }

    /// One line per span (p50 / p99 / max in ms), then the counters.
    pub fn summary(&self) -> String {
        let ms = |d: Duration| d.as_secs_f64() * 1000.0;
        let mut text = String::new();
        for (name, from, to) in SPANS {
            if let Some(s) = self.span(from, to) {
                let _ = writeln!(
                    text,
                    "{name}: p50 {:.3} ms, p99 {:.3} ms, max {:.3} ms ({} frames)",
                    ms(s.p50),
                    ms(s.p99),
                    ms(s.max),
                    s.count
                );
            }
        }
        let _ = write!(
            text,
            "decoded {}, presented {}, dropped {}, corrupt {}",
            self.decoded, self.presented, self.dropped, self.corrupt
        );
        text
    }
}

#[cfg(test)]
#[path = "stats_tests.rs"]
mod tests;
