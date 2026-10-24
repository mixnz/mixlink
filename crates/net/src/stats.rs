//! Transport counters: relaxed atomics written on the hot path, read as plain snapshots.

use std::sync::atomic::{AtomicU64, Ordering};

/// Per-lane counters, shared by the lane ends and the transport.
#[derive(Debug, Default)]
pub struct LaneCounters {
    /// Datagrams committed into the lane.
    pub passed: AtomicU64,
    /// `try_reserve` calls that found the lane full.
    pub full: AtomicU64,
}

impl LaneCounters {
    pub fn snapshot(&self) -> LaneCount {
        LaneCount {
            passed: self.passed.load(Ordering::Relaxed),
            full: self.full.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LaneCount {
    pub passed: u64,
    pub full: u64,
}
