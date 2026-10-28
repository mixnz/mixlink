//! Decoded frames waiting for the swapchain (spec 0020): at most two, shown oldest first. Frames that
//! arrive bunched into one refresh are each shown instead of the older one being dropped (that drop
//! read as judder at 60 fps on a 60 Hz client); a third gives up the oldest, which bounds latency.

/// Frames kept; the decoder pool holds these plus the one on screen.
const DEPTH: usize = 2;

#[derive(Debug)]
pub struct FrameQueue<T> {
    slots: [Option<T>; DEPTH],
    head: usize,
    len: usize,
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
