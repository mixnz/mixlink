//! Seal / open cost per full datagram (1216 B + tag), spec 0018 → Latency impact.
//! Run by hand: `cargo test -p mixlink-proto --release --test crypto_bench -- --ignored --nocapture`.

use std::time::{Duration, Instant};

use mixlink_core::ids::{FrameId, Seq};
use mixlink_proto::channel::Channel;
use mixlink_proto::crypto::{KEY_LEN, Role, SessionKeys, TAG_LEN};
use mixlink_proto::header::{Flags, HEADER_LEN, MAX_DATAGRAM, PacketHeader};

const ROUNDS: usize = 100_000;

fn percentiles(name: &str, mut samples: Vec<Duration>) {
    samples.sort();
    let total: Duration = samples.iter().sum();
    let mb_s = (ROUNDS * MAX_DATAGRAM) as f64 / total.as_secs_f64() / 1e6;
    eprintln!(
        "{name}: p50 {} ns, p99 {} ns, max {} ns, {mb_s:.0} MB/s",
        samples[ROUNDS / 2].as_nanos(),
        samples[ROUNDS * 99 / 100].as_nanos(),
        samples[ROUNDS - 1].as_nanos()
    );
}

#[test]
#[ignore = "benchmark, run by hand"]
fn seal_open_per_datagram() {
    let keys = SessionKeys {
        c2h: [5; KEY_LEN],
        h2c: [6; KEY_LEN],
    };
    let mut sealer = keys.sealer(Role::Host, Channel::Video);
    let mut opener = keys.opener(Role::Client, Channel::Video);
    let mut buf = [0x33u8; MAX_DATAGRAM + TAG_LEN];
    let mut seal_times = Vec::with_capacity(ROUNDS);
    let mut open_times = Vec::with_capacity(ROUNDS);
    for i in 0..ROUNDS {
        let mut head = [0u8; HEADER_LEN];
        PacketHeader {
            channel: Channel::Video,
            flags: Flags::empty(),
            seq: Seq::new(i as u32),
            frame: FrameId::new(0),
            frag_index: 0,
            frag_count: 1,
        }
        .encode(&mut head);
        buf[..HEADER_LEN].copy_from_slice(&head);
        let start = Instant::now();
        let total = sealer.seal(&mut buf, MAX_DATAGRAM).unwrap();
        let sealed = Instant::now();
        opener.open(&mut buf[..total]).unwrap();
        let opened = Instant::now();
        seal_times.push(sealed - start);
        open_times.push(opened - sealed);
    }
    percentiles("seal", seal_times);
    percentiles("open", open_times);
}
