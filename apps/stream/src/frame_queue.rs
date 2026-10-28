//! Decoded frames waiting for the swapchain (spec 0020): at most three, shown oldest first. Frames that
//! arrive bunched into one refresh are each shown instead of one being dropped (judder at 60 fps on a
//! 60 Hz client). Host and client clocks run at nearly the same rate, so a backlog never drains by itself:
//! after half a second of backlog one frame is skipped on purpose, which keeps latency low and leaves
//! room for the next bunch.

/// Frames kept; the decoder pool holds these plus the one on screen (`EXTRA_SURFACES` = 4).
const DEPTH: usize = 3;
/// Refreshes in a row with a frame still waiting behind the one shown before one is skipped.
const DRAIN_AFTER: u32 = 30;

#[derive(Debug)]
pub struct FrameQueue<T> {
    slots: [Option<T>; DEPTH],
    head: usize,
    len: usize,
    /// Refreshes in a row that found a backlog.
    backlog: u32,
}

impl<T> Default for FrameQueue<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> FrameQueue<T> {
    pub fn new() -> Self {
        Self {
            slots: [const { None }; DEPTH],
            head: 0,
            len: 0,
            backlog: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Adds the newest frame; returns the oldest when the queue was full (dropped).
    // HOT PATH
    pub fn push(&mut self, frame: T) -> Option<T> {
        let dropped = if self.len == DEPTH { self.pop() } else { None };
        let tail = (self.head + self.len) % DEPTH;
        self.slots[tail] = Some(frame);
        self.len += 1;
        dropped
    }

    /// The frame to show at this refresh, and a frame skipped to drain a lasting backlog.
    // HOT PATH
    pub fn pop_paced(&mut self) -> (Option<T>, Option<T>) {
        self.backlog = if self.len >= 2 { self.backlog + 1 } else { 0 };
        let skipped = if self.backlog >= DRAIN_AFTER {
            self.backlog = 0;
            self.pop()
        } else {
            None
        };
        (self.pop(), skipped)
    }

    /// The oldest frame.
    // HOT PATH
    pub fn pop(&mut self) -> Option<T> {
        if self.len == 0 {
            return None;
        }
        let frame = self.slots[self.head].take();
        self.head = (self.head + 1) % DEPTH;
        self.len -= 1;
        frame
    }
}

#[cfg(test)]
#[path = "frame_queue_tests.rs"]
mod tests;
