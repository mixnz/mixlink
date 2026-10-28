//! Frame gaps of one stream (spec 0020): DWM sends frames only on change, so a long gap is normal on a
//! still screen. A stall is a long gap right after a run of steady frames (video playing): those are
//! kept with their time into the session, for the log at the end.

use std::time::Duration;

use mixlink_core::clock::Ticks;

/// Frames this close count as a steady run.
const STEADY: Duration = Duration::from_millis(50);
/// A run this long makes the next long gap a stall.
const STEADY_RUN: u32 = 30;
/// A gap at least this long after a steady run is a stall.
const STALL: Duration = Duration::from_millis(250);
/// Stalls kept for the log.
const KEPT: usize = 16;

/// One stall: when it started (ms into the stream) and how long it lasted (ms).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Stall {
    pub at_ms: u64,
    pub gap_ms: u64,
}

#[derive(Debug, Default)]
pub struct GapStats {
    first: Option<Ticks>,
    last: Option<Ticks>,
    run: u32,
    longest: Duration,
    stalls: [Stall; KEPT],
    count: usize,
}

impl GapStats {
    /// One frame acquired at `at`. No allocation.
    // HOT PATH
    pub fn frame(&mut self, at: Ticks) {
        let first = *self.first.get_or_insert(at);
        if let Some(last) = self.last.replace(at) {
            let gap = at.duration_since(last);
            self.longest = self.longest.max(gap);
            if gap >= STALL && self.run >= STEADY_RUN {
                if let Some(slot) = self.stalls.get_mut(self.count) {
                    *slot = Stall {
                        at_ms: millis(last.duration_since(first)),
                        gap_ms: millis(gap),
                    };
                }
                self.count += 1;
            }
            self.run = if gap < STEADY { self.run + 1 } else { 0 };
        }
    }

    pub fn longest(&self) -> Duration {
        self.longest
    }

    /// Every stall seen (may exceed the ones kept).
    pub fn stall_count(&self) -> usize {
        self.count
    }

    /// The first stalls, oldest first.
    pub fn stalls(&self) -> &[Stall] {
        &self.stalls[..self.count.min(KEPT)]
    }
}

fn millis(d: Duration) -> u64 {
    u64::try_from(d.as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
#[path = "gaps_tests.rs"]
mod tests;
