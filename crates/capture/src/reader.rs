//! What the reader does with the header on each wake-up (pure; the Windows source maps the real one).

use crate::header::{FrameSlot, MAGIC, VERSION, slot_index};

/// Read access to a frame header; the mapped section in production, a fake in tests.
pub trait HeaderView {
    fn magic(&self) -> u32;
    fn version(&self) -> u32;
    fn generation(&self) -> u32;
    fn size(&self) -> (u32, u32);
    fn latest_seq(&self) -> u64;
    fn slot(&self, index: usize) -> FrameSlot;
    fn slot_seq(&self, index: usize) -> u64;
}

/// Result of one `ReaderState::poll`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Poll {
    /// Nothing new (or the newest slot is being written).
    Idle,
    /// A new frame: the slot as read, with its seq.
    Frame { seq: u64, slot: FrameSlot },
    /// The driver published a new ring (first sight, mode change, new swapchain): reopen everything.
    Reopen {
        generation: u32,
        width: u32,
        height: u32,
    },
    /// No swapchain shared, or a header this reader does not understand.
    Gone,
}

/// Remembers the generation in use and the last frame handed out.
#[derive(Debug, Default)]
pub struct ReaderState {
    generation: Option<u32>,
    delivered: u64,
}

impl ReaderState {
    pub const fn new() -> Self {
        Self {
            generation: None,
            delivered: 0,
        }
    }

    // HOT PATH
    pub fn poll(&mut self, h: &impl HeaderView) -> Poll {
        if h.magic() != MAGIC || h.version() != VERSION {
            self.generation = None;
            return Poll::Gone;
        }
        let generation = h.generation();
        if self.generation != Some(generation) {
            self.generation = Some(generation);
            let (width, height) = h.size();
            return Poll::Reopen {
                generation,
                width,
                height,
            };
        }
        let latest = h.latest_seq();
        if latest == 0 || latest == self.delivered {
            return Poll::Idle;
        }
        let index = slot_index(latest);
        let slot = h.slot(index);
        // Re-read the seq: the driver zeroes it while rewriting the slot.
        if slot.seq != latest || h.slot_seq(index) != latest {
            return Poll::Idle;
        }
        self.delivered = latest;
        Poll::Frame { seq: latest, slot }
    }
}

#[cfg(test)]
#[path = "reader_tests.rs"]
mod tests;
