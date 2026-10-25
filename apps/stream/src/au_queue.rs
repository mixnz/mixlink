//! SPSC queue of access units, feeder (file now, network later) → video thread.
//!
//! Buffers are allocated once by `channel` and then only move between two `rtrb` rings: **filled**
//! (sender → receiver) and **free** (receiver → sender). Sending copies the AU into a free buffer
//! and stamps the receive time; nothing on the hot path allocates or blocks.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::thread::{self, Thread};

use mixlink_core::clock::{self, Ticks};
use rtrb::{Consumer, Producer, RingBuffer};

/// Largest access unit (a 4 MiB keyframe, the video reassembler's limit).
pub const AU_CAPACITY: usize = 4 * 1024 * 1024;
/// Buffers in flight: about 66 ms of video at 120 fps.
pub const AU_SLOTS: usize = 8;

#[derive(Debug)]
struct AuBuf {
    data: Box<[u8]>,
    len: usize,
    received: Ticks,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendError {
    /// Every buffer is waiting for the receiver; the AU is dropped.
    Full,
    /// Larger than the buffer capacity; the AU is dropped.
    Oversize,
}

/// Dropped AUs since start.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct QueueCounts {
    pub full: u64,
    pub oversize: u64,
}

#[derive(Debug, Default)]
struct Shared {
    full: AtomicU64,
    oversize: AtomicU64,
    wake: OnceLock<Thread>,
}

#[derive(Debug)]
pub struct AuSender {
    filled: Producer<Box<AuBuf>>,
    free: Consumer<Box<AuBuf>>,
    shared: Arc<Shared>,
    capacity: usize,
}

#[derive(Debug)]
pub struct AuReceiver {
    filled: Consumer<Box<AuBuf>>,
    free: Producer<Box<AuBuf>>,
    shared: Arc<Shared>,
}

/// `slots` buffers of `capacity` bytes each, allocated now.
pub fn channel(slots: usize, capacity: usize) -> (AuSender, AuReceiver) {
    let slots = slots.max(1);
    let (filled_tx, filled_rx) = RingBuffer::new(slots);
    let (mut free_tx, free_rx) = RingBuffer::new(slots);
    for _ in 0..slots {
        let buf = Box::new(AuBuf {
            data: vec![0; capacity].into_boxed_slice(),
            len: 0,
            received: Ticks::ZERO,
        });
        // Cannot fail: the ring holds exactly `slots`.
        let _ = free_tx.push(buf);
    }
    let shared = Arc::new(Shared::default());
    (
        AuSender {
            filled: filled_tx,
            free: free_rx,
            shared: Arc::clone(&shared),
            capacity,
        },
        AuReceiver {
            filled: filled_rx,
            free: free_tx,
            shared,
        },
    )
}

impl AuSender {
    /// Copies `au` into a free buffer, stamps it and wakes the receiver.
    // HOT PATH
    pub fn send(&mut self, au: &[u8]) -> Result<(), SendError> {
        if au.len() > self.capacity {
            self.shared.oversize.fetch_add(1, Ordering::Relaxed);
            return Err(SendError::Oversize);
        }
        let Ok(mut buf) = self.free.pop() else {
            self.shared.full.fetch_add(1, Ordering::Relaxed);
            return Err(SendError::Full);
        };
        buf.data[..au.len()].copy_from_slice(au);
        buf.len = au.len();
        buf.received = clock::now();
        // Cannot fail: as many buffers as ring slots.
        let _ = self.filled.push(buf);
        if let Some(thread) = self.shared.wake.get() {
            thread.unpark();
        }
        Ok(())
    }

    pub fn counts(&self) -> QueueCounts {
        QueueCounts {
            full: self.shared.full.load(Ordering::Relaxed),
            oversize: self.shared.oversize.load(Ordering::Relaxed),
        }
    }
}

impl AuReceiver {
    /// The calling thread is unparked on every send.
    pub fn attach_current_thread(&self) {
        let _ = self.shared.wake.set(thread::current());
    }

    // HOT PATH
    pub fn try_recv(&mut self) -> Option<Au<'_>> {
        let buf = self.filled.pop().ok()?;
        Some(Au {
            buf: Some(buf),
            free: &mut self.free,
        })
    }
}

/// One received access unit; its buffer goes back to the sender on drop.
#[derive(Debug)]
pub struct Au<'a> {
    buf: Option<Box<AuBuf>>,
    free: &'a mut Producer<Box<AuBuf>>,
}

impl Au<'_> {
    pub fn data(&self) -> &[u8] {
        self.buf.as_ref().map_or(&[], |b| &b.data[..b.len])
    }

    /// When the sender queued it (`clock::now`).
    pub fn received(&self) -> Ticks {
        self.buf.as_ref().map_or(Ticks::ZERO, |b| b.received)
    }
}

impl Drop for Au<'_> {
    fn drop(&mut self) {
        if let Some(buf) = self.buf.take() {
            // Cannot fail: as many buffers as ring slots.
            let _ = self.free.push(buf);
        }
    }
}

#[cfg(test)]
#[path = "au_queue_tests.rs"]
mod tests;
