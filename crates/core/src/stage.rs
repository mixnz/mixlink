//! Per-frame stage timestamps. Host and client stages live in separate records on separate
//! machines; this module never subtracts ticks across machines.

use std::time::Duration;

use crate::clock::{self, Ticks};
use crate::ids::FrameId;

/// Pipeline stages a frame passes through; host first, then client.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Stage {
    Captured,
    EncodeStart,
    EncodeEnd,
    Sent,
    Received,
    DecodeStart,
    DecodeEnd,
    Presented,
}

impl Stage {
    pub const COUNT: usize = 8;

    /// Every stage in declaration order (`ALL[s.index()] == s`).
    pub const ALL: [Stage; Stage::COUNT] = [
        Stage::Captured,
        Stage::EncodeStart,
        Stage::EncodeEnd,
        Stage::Sent,
        Stage::Received,
        Stage::DecodeStart,
        Stage::DecodeEnd,
        Stage::Presented,
    ];

    pub const fn index(self) -> usize {
        self as usize
    }
}

/// Timestamps of one frame; `Ticks::ZERO` means the stage was not reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StageStamps {
    frame: FrameId,
    stamps: [Ticks; Stage::COUNT],
}

impl StageStamps {
    pub const fn new(frame: FrameId) -> Self {
        Self {
            frame,
            stamps: [Ticks::ZERO; Stage::COUNT],
        }
    }

    pub const fn frame(&self) -> FrameId {
        self.frame
    }

    // HOT PATH
    #[inline]
    pub fn mark(&mut self, stage: Stage) {
        self.mark_at(stage, clock::now());
    }

    // HOT PATH
    #[inline]
    pub fn mark_at(&mut self, stage: Stage, ticks: Ticks) {
        self.stamps[stage.index()] = ticks;
    }

    pub fn get(&self, stage: Stage) -> Option<Ticks> {
        let ticks = self.stamps[stage.index()];
        (ticks != Ticks::ZERO).then_some(ticks)
    }

    /// Time from `from` to `to`; `None` if either is unmarked or `to` is earlier.
    pub fn span(&self, from: Stage, to: Stage) -> Option<Duration> {
        let start = self.get(from)?;
        let end = self.get(to)?;
        (end >= start).then(|| end.duration_since(start))
    }
}

#[cfg(test)]
#[path = "stage_tests.rs"]
mod tests;
