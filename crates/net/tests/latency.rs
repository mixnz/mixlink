//! One datagram through tx lane → send thread → loopback → recv thread → rx lane.
//! Prints (stderr) p50 / p99 / max (`--nocapture`); asserts p99 ≤ 250 µs only in release builds.

use std::time::Duration;

use mixlink_core::clock::{self, Ticks};
use mixlink_core::ids::{FrameId, Seq};
use mixlink_net::{Transport, TransportConfig};
use mixlink_proto::channel::Channel;
use mixlink_proto::header::{Flags, HEADER_LEN, PacketHeader};

const ROUNDS: usize = 10_000;

#[test]
fn one_datagram_latency() {
    let (_a, mut a_lanes) = Transport::start(TransportConfig::loopback()).unwrap();
    let (b, mut b_lanes) = Transport::start(TransportConfig::loopback()).unwrap();
    let mut tx = a_lanes.take_tx(Channel::Input).unwrap();
    let mut rx = b_lanes.take_rx(Channel::Input).unwrap();
    let mut head = [0u8; HEADER_LEN];
    PacketHeader {
        channel: Channel::Input,
        flags: Flags::empty(),
        seq: Seq::new(0),
        frame: FrameId::new(0),
        frag_index: 0,
        frag_count: 1,
    }
    .encode(&mut head);

    let mut samples = Vec::with_capacity(ROUNDS);
    for _ in 0..ROUNDS {
        let mut slot = tx.try_reserve().unwrap();
        let buf = slot.buf();
        buf[..HEADER_LEN].copy_from_slice(&head);
        let sent = clock::now();
        buf[HEADER_LEN..HEADER_LEN + 8].copy_from_slice(&sent.raw().to_le_bytes());
        slot.commit(HEADER_LEN + 8, b.local_addr());
        let got = rx.recv_timeout(Duration::from_secs(1)).unwrap();
        let stamp: [u8; 8] = got.data()[HEADER_LEN..].try_into().unwrap();
        let sent = Ticks::from_raw(u64::from_le_bytes(stamp));
        samples.push(got.at().duration_since(sent));
    }
    samples.sort();
    let p50 = samples[ROUNDS / 2];
    let p99 = samples[ROUNDS * 99 / 100];
    let max = samples[ROUNDS - 1];
    eprintln!(
        "one datagram commit -> rx: p50 {} us, p99 {} us, max {} us",
        p50.as_micros(),
        p99.as_micros(),
        max.as_micros()
    );
    if !cfg!(debug_assertions) {
        assert!(p99 <= Duration::from_micros(250), "p99 {p99:?}");
    }
}
