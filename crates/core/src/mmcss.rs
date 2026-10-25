//! MMCSS thread priority for latency-critical threads (specs 0005, 0007, 0013).

/// MMCSS task class the calling thread joins.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MmcssTask {
    /// "Pro Audio" at critical priority: network send / recv threads.
    ProAudio,
    /// "Games" at high priority: the client video thread (decode + present).
    Games,
}

pub use crate::platform::Mmcss;

#[cfg(test)]
#[path = "mmcss_tests.rs"]
mod tests;
