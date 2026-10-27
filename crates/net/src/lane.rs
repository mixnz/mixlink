//! Per-channel SPSC lanes of preallocated datagram slots.
//!
//! Each lane moves `Box<Slot>` between two `rtrb` rings: **filled** (writer → reader) and **free**
//! (reader → writer). Each end also keeps one staged slot of its own: the writer fills its staged
//! slot and swaps it for a free one on commit; the reader swaps a filled slot in and hands its old
//! one back. All boxes exist from `pair` on, so the hot path never allocates or copies a slot.

use std::net::{Ipv4Addr, SocketAddrV4};
use std::sync::atomic::Ordering;
use std::sync::{Arc, OnceLock};
use std::thread::{self, Thread};
use std::time::{Duration, Instant};

use mixlink_core::clock::{self, Ticks};
use mixlink_proto::crypto::{Opener, Sealer};
use mixlink_proto::error::CryptoError;
use mixlink_proto::header::MAX_DATAGRAM;
use rtrb::{Consumer, Producer, RingBuffer};

use crate::stats::LaneCounters;

/// One datagram buffer: a full v1 datagram plus 16 bytes of room for the AES-GCM tag.
pub const SLOT_LEN: usize = MAX_DATAGRAM + 16;

#[derive(Debug)]
pub(crate) struct Slot {
    buf: [u8; SLOT_LEN],
    len: u16,
    peer: SocketAddrV4,
    at: Ticks,
}

impl Slot {
    fn boxed() -> Box<Slot> {
        Box::new(Slot {
            buf: [0; SLOT_LEN],
            len: 0,
            peer: SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0),
            at: Ticks::ZERO,
        })
    }
}

/// The thread to unpark when a lane gets data; registered once by that thread.
#[derive(Debug, Default)]
pub(crate) struct Wake(OnceLock<Thread>);

impl Wake {
    pub(crate) fn attach_current(&self) {
        if self.0.get().is_none() {
            let _ = self.0.set(thread::current());
        }
    }

    // HOT PATH
    pub(crate) fn wake(&self) {
        if let Some(thread) = self.0.get() {
            thread.unpark();
        }
    }
}

/// Builds one lane of `capacity` slots (at least 1).
pub(crate) fn pair(
    capacity: usize,
    consumer_wake: Arc<Wake>,
    counters: Arc<LaneCounters>,
) -> (Writer, Reader) {
    let capacity = capacity.max(1);
    let (mut free_tx, free_rx) = RingBuffer::new(capacity);
    let (filled_tx, filled_rx) = RingBuffer::new(capacity);
    for _ in 0..capacity {
        // Cannot fail: exactly `capacity` pushes into an empty ring of `capacity`.
        let _ = free_tx.push(Slot::boxed());
    }
    let writer = Writer {
        stage: Slot::boxed(),
        free: free_rx,
        filled: filled_tx,
        wake: consumer_wake,
        counters,
    };
    let reader = Reader {
        stage: Slot::boxed(),
        filled: filled_rx,
        free: free_tx,
        wake: Arc::clone(&writer.wake),
    };
    (writer, reader)
}

#[derive(Debug)]
pub(crate) struct Writer {
    stage: Box<Slot>,
    free: Consumer<Box<Slot>>,
    filled: Producer<Box<Slot>>,
    wake: Arc<Wake>,
    counters: Arc<LaneCounters>,
}

impl Writer {
    // HOT PATH
    pub(crate) fn try_reserve(&mut self) -> Option<Reserved<'_>> {
        if self.free.is_empty() {
            self.counters.full.fetch_add(1, Ordering::Relaxed);
            return None;
        }
        Some(Reserved { writer: self })
    }
}

/// A reserved slot; dropping it without `commit` leaves it staged for the next reserve.
#[derive(Debug)]
pub(crate) struct Reserved<'a> {
    writer: &'a mut Writer,
}

impl Reserved<'_> {
    pub(crate) fn buf(&mut self) -> &mut [u8; SLOT_LEN] {
        &mut self.writer.stage.buf
    }

    pub(crate) fn datagram(&mut self) -> &mut [u8; MAX_DATAGRAM] {
        const { assert!(SLOT_LEN >= MAX_DATAGRAM) };
        match self.writer.stage.buf.first_chunk_mut::<MAX_DATAGRAM>() {
            Some(datagram) => datagram,
            None => unreachable!("SLOT_LEN >= MAX_DATAGRAM is asserted at compile time"),
        }
    }

    // HOT PATH
    pub(crate) fn commit(self, len: usize, peer: SocketAddrV4, at: Ticks) {
        debug_assert!(len <= SLOT_LEN, "datagram of {len} bytes");
        let writer = self.writer;
        // `try_reserve` saw a free slot and only this end pops `free`, so it is still there.
        let Ok(fresh) = writer.free.pop() else {
            writer.counters.full.fetch_add(1, Ordering::Relaxed);
            return;
        };
        let mut full = std::mem::replace(&mut writer.stage, fresh);
        full.len = u16::try_from(len.min(SLOT_LEN)).unwrap_or(0);
        full.peer = peer;
        full.at = at;
        // Cannot fail: boxes in the two rings never exceed the ring capacity.
        let _ = writer.filled.push(full);
        writer.counters.passed.fetch_add(1, Ordering::Relaxed);
        writer.wake.wake();
    }
}

#[derive(Debug)]
pub(crate) struct Reader {
    stage: Box<Slot>,
    filled: Consumer<Box<Slot>>,
    free: Producer<Box<Slot>>,
    wake: Arc<Wake>,
}

impl Reader {
    // HOT PATH
    pub(crate) fn try_pop(&mut self) -> Option<Received<'_>> {
        let next = self.filled.pop().ok()?;
        let done = std::mem::replace(&mut self.stage, next);
        // Cannot fail: boxes in the two rings never exceed the ring capacity.
        let _ = self.free.push(done);
        Some(Received {
            slot: &mut self.stage,
        })
    }

    pub(crate) fn attach_current_thread(&self) {
        self.wake.attach_current();
    }

    pub(crate) fn recv_timeout(&mut self, timeout: Duration) -> Option<Received<'_>> {
        self.attach_current_thread();
        // `None` = no deadline (the timeout is too large to represent).
        let deadline = Instant::now().checked_add(timeout);
        while self.filled.is_empty() {
            match deadline {
                Some(deadline) => {
                    let now = Instant::now();
                    if now >= deadline {
                        return None;
                    }
                    thread::park_timeout(deadline - now);
                }
                None => thread::park(),
            }
        }
        self.try_pop()
    }
}

/// A received datagram; valid until the next pop on the same lane.
#[derive(Debug)]
pub(crate) struct Received<'a> {
    slot: &'a mut Slot,
}

impl Received<'_> {
    pub(crate) fn data(&self) -> &[u8] {
        &self.slot.buf[..usize::from(self.slot.len)]
    }

    pub(crate) fn data_mut(&mut self) -> &mut [u8] {
        &mut self.slot.buf[..usize::from(self.slot.len)]
    }

    /// The datagram is now `len` bytes (it was opened in place).
    pub(crate) fn truncate(&mut self, len: usize) {
        self.slot.len = self.slot.len.min(u16::try_from(len).unwrap_or(u16::MAX));
    }

    pub(crate) fn peer(&self) -> SocketAddrV4 {
        self.slot.peer
    }

    pub(crate) fn at(&self) -> Ticks {
        self.slot.at
    }
}

/// Application end of a send lane. One owner thread; `Send`, not `Clone`.
#[derive(Debug)]
pub struct TxLane(pub(crate) Writer);

impl TxLane {
    // HOT PATH
    /// A free slot, or `None` when the lane is full (counted; never blocks).
    pub fn try_reserve(&mut self) -> Option<TxSlot<'_>> {
        self.0.try_reserve().map(TxSlot)
    }
}

/// A slot to fill; `commit` queues it, dropping it keeps it for the next reserve.
#[derive(Debug)]
pub struct TxSlot<'a>(Reserved<'a>);

impl TxSlot<'_> {
    /// Whole slot, including the 16 bytes of tag room.
    pub fn buf(&mut self) -> &mut [u8; SLOT_LEN] {
        self.0.buf()
    }

    /// First `MAX_DATAGRAM` bytes: the shape `proto::packetizer::Fragments::next_into` writes.
    pub fn datagram(&mut self) -> &mut [u8; MAX_DATAGRAM] {
        self.0.datagram()
    }

    // HOT PATH
    pub fn commit(self, len: usize, peer: SocketAddrV4) {
        self.0.commit(len, peer, clock::now());
    }

    /// Seals the `len`-byte datagram in `buf()` (spec 0018), then commits it. On error nothing is
    /// queued and the slot stays staged for the next reserve.
    // HOT PATH
    pub fn seal_commit(
        mut self,
        sealer: &mut Sealer,
        len: usize,
        peer: SocketAddrV4,
    ) -> Result<(), CryptoError> {
        let total = sealer.seal(self.0.buf(), len)?;
        self.0.commit(total, peer, clock::now());
        Ok(())
    }
}

/// Application end of a receive lane. One owner thread; `Send`, not `Clone`.
#[derive(Debug)]
pub struct RxLane(pub(crate) Reader);

impl RxLane {
    // HOT PATH
    pub fn try_recv(&mut self) -> Option<RxDatagram<'_>> {
        self.0.try_pop().map(RxDatagram)
    }

    /// Parks until a datagram arrives or `timeout` ends; attaches the calling thread.
    pub fn recv_timeout(&mut self, timeout: Duration) -> Option<RxDatagram<'_>> {
        self.0.recv_timeout(timeout).map(RxDatagram)
    }

    /// Makes the calling thread the one woken for this lane, so one thread can serve several
    /// lanes with `try_recv` + `std::thread::park_timeout`. The first attach wins.
    pub fn attach_current_thread(&self) {
        self.0.attach_current_thread();
    }
}

/// A received datagram, valid until the next receive on its lane.
#[derive(Debug)]
pub struct RxDatagram<'a>(Received<'a>);

impl RxDatagram<'_> {
    pub fn data(&self) -> &[u8] {
        self.0.data()
    }

    pub fn peer(&self) -> SocketAddrV4 {
        self.0.peer()
    }

    /// Receive time (`clock::now()` on the recv thread).
    pub fn at(&self) -> Ticks {
        self.0.at()
    }

    /// Opens the sealed datagram in place (spec 0018); afterwards `data()` is the plaintext
    /// datagram. On error `data()` is undefined and the datagram must be dropped.
    // HOT PATH
    pub fn open(&mut self, opener: &mut Opener) -> Result<&[u8], CryptoError> {
        let len = opener.open(self.0.data_mut())?;
        self.0.truncate(len);
        Ok(self.0.data())
    }
}

#[cfg(test)]
#[path = "lane_tests.rs"]
mod tests;
