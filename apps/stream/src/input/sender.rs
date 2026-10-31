//! Client input to the host (spec 0021): each new state goes out at once, sealed, on the Input lane; the
//! same state goes again 4 ms and 16 ms later, and every 100 ms while a key or button is held, so one
//! lost datagram costs at most a few milliseconds and the host never times out a held key.

use std::net::SocketAddrV4;
use std::time::{Duration, Instant};

use mixlink_core::ids::FrameId;
use mixlink_core::input::InputPacket;
use mixlink_net::TxLane;
use mixlink_proto::channel::Channel;
use mixlink_proto::crypto::{Role, Sealer, SessionKeys};
use mixlink_proto::header::Flags;
use mixlink_proto::input::{INPUT_LEN, encode};
use mixlink_proto::packetizer::Packetizer;

use crate::net::drop_knob::{DropKnob, DropLane};

/// Repeats of a new state, after it was sent.
const REPEATS: [Duration; 2] = [Duration::from_millis(4), Duration::from_millis(16)];
/// While anything is held; the host releases after 500 ms of silence.
pub const KEEPALIVE: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SendCounts {
    /// Datagrams handed to the lane (repeats included).
    pub sent: u64,
    pub repeats: u64,
    /// Dropped by `--drop input=`.
    pub dropped: u64,
    pub lane_full: u64,
}

/// The sealed Input lane to the host.
#[derive(Debug)]
pub struct InputTx {
    tx: TxLane,
    sealer: Sealer,
    packetizer: Packetizer,
    host: SocketAddrV4,
    message: FrameId,
    drop: DropKnob,
    counts: SendCounts,
}

impl InputTx {
    pub fn new(tx: TxLane, keys: &SessionKeys, host: SocketAddrV4, drop: DropKnob) -> InputTx {
        InputTx {
            tx,
            sealer: keys.sealer(Role::Client, Channel::Input),
            packetizer: Packetizer::new(Channel::Input),
            host,
            message: FrameId::new(0),
            drop,
            counts: SendCounts::default(),
        }
    }

    /// `false` when the lane is full (the next repeat carries the state).
    // HOT PATH
    pub fn send(&mut self, packet: &InputPacket, repeat: bool) -> bool {
        if self.drop.should_drop(DropLane::Input) {
            self.counts.dropped += 1;
            return true;
        }
        let mut body = [0u8; INPUT_LEN];
        encode(packet, &mut body);
        let message = self.message;
        self.message = message.next();
        let Ok(mut fragments) = self.packetizer.packetize(message, Flags::empty(), &body) else {
            return false;
        };
        let Some(mut slot) = self.tx.try_reserve() else {
            self.counts.lane_full += 1;
            return false;
        };
        let Some(len) = fragments.next_into(slot.datagram()) else {
            return false;
        };
        if slot.seal_commit(&mut self.sealer, len, self.host).is_err() {
            return false;
        }
        self.counts.sent += 1;
        self.counts.repeats += u64::from(repeat);
        true
    }

    pub fn counts(&self) -> SendCounts {
        self.counts
    }
}

/// When the current state goes out again.
#[derive(Debug, Clone, Copy, Default)]
pub struct Schedule {
    /// When the newest state was first sent.
    new_at: Option<Instant>,
    /// Repeats of it done.
    repeats: usize,
    last_sent: Option<Instant>,
}

impl Schedule {
    /// A new state was just sent.
    pub fn on_send(&mut self, now: Instant) {
        self.new_at = Some(now);
        self.repeats = 0;
        self.last_sent = Some(now);
    }

    /// The next repeat, if any.
    pub fn next(&self, held: bool) -> Option<Instant> {
        let new_at = self.new_at?;
        if let Some(after) = REPEATS.get(self.repeats) {
            return Some(new_at + *after);
        }
        let last = self.last_sent?;
        held.then_some(last + KEEPALIVE)
    }

    /// `true`: a repeat is due at `now` (and counted as sent).
    pub fn fire(&mut self, now: Instant, held: bool) -> bool {
        let Some(at) = self.next(held) else {
            return false;
        };
        if now < at {
            return false;
        }
        self.repeats = (self.repeats + 1).min(REPEATS.len());
        self.last_sent = Some(now);
        true
    }
}

#[cfg(test)]
#[path = "sender_tests.rs"]
mod tests;
