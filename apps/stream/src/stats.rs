//! Per-frame stage timings of the last N frames, summarized when the window closes.

use std::fmt::Write as _;
use std::time::Duration;

use mixlink_core::clock::Ticks;
use mixlink_core::ids::FrameId;
use mixlink_core::ipc::{LatencySummary, SpanSummary};
use mixlink_core::stage::{Stage, StageStamps};

use crate::au_queue::AuInfo;

/// Frames kept for the summary.
pub const STATS_FRAMES: usize = 10_000;

/// How one latency span is read from a frame (spec 0020).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpanKind {
    /// Host stage from `FrameMeta`: 0 capture, 1 encode, 2 send.
    Host(usize),
    /// Half the median RTT.
    Network,
    /// First → last fragment.
    Reassemble,
    /// Last fragment (play mode: queued) → decode start.
    Queue,
    Stages(Stage, Stage),
    /// Host + network + first fragment → presented.
    Total,
}

/// Spans in the summary and in `LatencySummary`, in pipeline order. Host and network spans are left
/// out in play mode (no host).
pub const LATENCY_SPANS: [(&str, SpanKind); 9] = [
    ("capture", SpanKind::Host(0)),
    ("encode", SpanKind::Host(1)),
    ("send", SpanKind::Host(2)),
    ("network", SpanKind::Network),
    ("reassemble", SpanKind::Reassemble),
    ("queue", SpanKind::Queue),
    (
        "decode",
        SpanKind::Stages(Stage::DecodeStart, Stage::DecodeEnd),
    ),
    (
        "present",
        SpanKind::Stages(Stage::DecodeEnd, Stage::Presented),
    ),
    ("total", SpanKind::Total),
];

fn us(v: u32) -> Option<Duration> {
    (v > 0).then(|| Duration::from_micros(u64::from(v)))
}

impl SpanKind {
    /// This span of one frame; `None` when the frame did not reach both ends.
    pub fn of(self, s: &StageStamps, info: &AuInfo) -> Option<Duration> {
        let last = (info.last != Ticks::ZERO).then_some(info.last);
        match self {
            SpanKind::Host(i) => us(*info.host_us.get(i)?),
            SpanKind::Network => us(info.network_us),
            SpanKind::Reassemble => Some(last?.duration_since(s.get(Stage::Received)?)),
            SpanKind::Queue => {
                let from = last.or(s.get(Stage::Received))?;
                Some(s.get(Stage::DecodeStart)?.duration_since(from))
            }
            SpanKind::Stages(from, to) => s.span(from, to),
            SpanKind::Total => {
                let host: u64 = info.host_us.iter().map(|v| u64::from(*v)).sum();
                let away = Duration::from_micros(host + u64::from(info.network_us));
                Some(s.span(Stage::Received, Stage::Presented)? + away)
            }
        }
    }
}

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
    ring: Box<[(StageStamps, AuInfo)]>,
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
            ring: vec![(empty, AuInfo::default()); capacity.max(1)].into_boxed_slice(),
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
        self.record_with(stamps, &AuInfo::default());
    }

    /// As `record`, with where the AU came from (network times, host stages).
    // HOT PATH
    pub fn record_with(&mut self, stamps: &StageStamps, info: &AuInfo) {
        self.ring[self.next] = (*stamps, *info);
        self.next = (self.next + 1) % self.ring.len();
        self.filled = (self.filled + 1).min(self.ring.len());
    }

    /// Nearest-rank percentiles of `from` → `to` over frames that reached both.
    pub fn span(&self, from: Stage, to: Stage) -> Option<SpanStats> {
        self.span_of(SpanKind::Stages(from, to))
    }

    /// Nearest-rank percentiles of one latency span over frames that have it.
    pub fn span_of(&self, kind: SpanKind) -> Option<SpanStats> {
        let mut spans: Vec<Duration> = self.ring[..self.filled]
            .iter()
            .filter_map(|(s, info)| kind.of(s, info))
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
        for (name, kind) in LATENCY_SPANS {
            if let Some(s) = self.span_of(kind) {
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

    /// The session's latency for the launcher (spec 0020): p50 / p99 of every span frames reached.
    pub fn latency_summary(&self) -> LatencySummary {
        let micros = |d: Duration| u32::try_from(d.as_micros()).unwrap_or(u32::MAX);
        let spans = LATENCY_SPANS
            .iter()
            .filter_map(|(name, kind)| {
                self.span_of(*kind).map(|s| SpanSummary {
                    name: (*name).to_owned(),
                    p50_us: micros(s.p50),
                    p99_us: micros(s.p99),
                })
            })
            .collect();
        LatencySummary {
            frames: self.presented,
            spans,
        }
    }
}

#[cfg(test)]
#[path = "stats_tests.rs"]
mod tests;
