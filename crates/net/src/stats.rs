//! Transport counters: relaxed atomics written on the hot path, read as plain snapshots.

use std::sync::atomic::{AtomicU64, Ordering};

use mixlink_proto::channel::Channel;

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

/// Socket-level counters, shared by the send and recv threads.
#[derive(Debug, Default)]
pub(crate) struct SocketCounters {
    pub(crate) send_err: AtomicU64,
    pub(crate) recv_err: AtomicU64,
    pub(crate) rx_bad: AtomicU64,
}

impl SocketCounters {
    // HOT PATH
    pub(crate) fn bump(counter: &AtomicU64) {
        counter.fetch_add(1, Ordering::Relaxed);
    }
}

/// Snapshot of every transport counter. `tx[..].passed` counts datagrams committed by the app,
/// `rx[..].passed` datagrams the recv thread put in a lane; lanes are indexed by `channel.id() - 1`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransportStats {
    pub tx: [LaneCount; 6],
    pub rx: [LaneCount; 6],
    pub send_err: u64,
    pub recv_err: u64,
    /// Datagrams dropped before any lane: too short or long, IPv6, unknown version or channel.
    pub rx_bad: u64,
    /// Socket buffer sizes Windows actually applied.
    pub recv_buffer: usize,
    pub send_buffer: usize,
    pub mmcss_send: bool,
    pub mmcss_recv: bool,
}

impl TransportStats {
    pub fn tx(&self, channel: Channel) -> LaneCount {
        self.tx[lane_index(channel)]
    }

    pub fn rx(&self, channel: Channel) -> LaneCount {
        self.rx[lane_index(channel)]
    }
}

/// Lane array index of a channel (ids 1-6 → 0-5).
pub(crate) const fn lane_index(channel: Channel) -> usize {
    channel.id() as usize - 1
}
