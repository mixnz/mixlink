//! Client → host feedback (spec 0020) and the session's round-trip time. Sealed feedback datagrams on
//! the Feedback lane; RTT from sealed `Ping` / `Pong` on the control link.

use std::net::SocketAddrV4;
use std::time::{Duration, Instant};

use mixlink_core::ids::FrameId;
use mixlink_net::TxLane;
use mixlink_proto::channel::Channel;
use mixlink_proto::crypto::{Role, Sealer, SessionKeys};
use mixlink_proto::feedback::{FEEDBACK_LEN, Feedback};
use mixlink_proto::header::Flags;
use mixlink_proto::packetizer::Packetizer;

/// RTT samples kept for the median.
const RTT_SAMPLES: usize = 16;
/// Pings in flight remembered (one per second; older ones count as lost).
const PINGS: usize = 8;

#[derive(Debug)]
pub struct FeedbackTx {
    tx: TxLane,
    sealer: Sealer,
    packetizer: Packetizer,
    host: SocketAddrV4,
    message: FrameId,
}

impl FeedbackTx {
    pub fn new(tx: TxLane, keys: &SessionKeys, host: SocketAddrV4) -> Self {
        Self {
            tx,
            sealer: keys.sealer(Role::Client, Channel::Feedback),
            packetizer: Packetizer::new(Channel::Feedback),
            host,
            message: FrameId::new(0),
        }
    }

    /// `false` when the lane is full (the caller asks again later).
    // HOT PATH
    pub fn send(&mut self, msg: Feedback) -> bool {
        let mut body = [0u8; FEEDBACK_LEN];
        let len = msg.encode(&mut body);
        let message = self.message;
        self.message = message.next();
        let Ok(mut fragments) = self
            .packetizer
            .packetize(message, Flags::empty(), &body[..len])
        else {
            return false;
        };
        let Some(mut slot) = self.tx.try_reserve() else {
            return false;
        };
        let Some(datagram_len) = fragments.next_into(slot.datagram()) else {
            return false;
        };
        slot.seal_commit(&mut self.sealer, datagram_len, self.host)
            .is_ok()
    }
}

/// Pings sent and the RTT samples of their pongs.
#[derive(Debug, Default)]
pub struct Rtt {
    sent: [Option<(u32, Instant)>; PINGS],
    samples: [u32; RTT_SAMPLES],
    count: usize,
    next: usize,
}

impl Rtt {
    pub fn sent(&mut self, n: u32, at: Instant) {
        self.sent[n as usize % PINGS] = Some((n, at));
    }

    /// Records the RTT of pong `n`; unknown or repeated pongs are ignored.
    pub fn pong(&mut self, n: u32, at: Instant) {
        let slot = &mut self.sent[n as usize % PINGS];
        let Some((sent_n, sent_at)) = *slot else {
            return;
        };
        if sent_n != n {
            return;
        }
        *slot = None;
        let us =
            u32::try_from(at.saturating_duration_since(sent_at).as_micros()).unwrap_or(u32::MAX);
        self.samples[self.next] = us;
        self.next = (self.next + 1) % RTT_SAMPLES;
        self.count = (self.count + 1).min(RTT_SAMPLES);
    }

    /// Median of the last 16 RTTs, in µs; 0 before the first pong.
    pub fn median_us(&self) -> u32 {
        if self.count == 0 {
            return 0;
        }
        let mut sorted = self.samples;
        let window = &mut sorted[..self.count];
        window.sort_unstable();
        window[self.count / 2]
    }

    /// One-way network time estimate: half the median RTT.
    pub fn one_way(&self) -> Duration {
        Duration::from_micros(u64::from(self.median_us() / 2))
    }
}

#[cfg(test)]
#[path = "feedback_tx_tests.rs"]
mod tests;
