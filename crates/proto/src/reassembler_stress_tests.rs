//! Fuzz and timing tests for the reassembler.

use std::time::Instant;

use super::*;
use crate::header::{HEADER_LEN, MAX_DATAGRAM};
use crate::packetizer::Packetizer;
use crate::test_util::Rng;

fn video() -> Reassembler {
    Reassembler::new(ReassemblerConfig::for_channel(Channel::Video))
}

#[test]
fn random_datagrams_never_panic() {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let mut r = video();
    let mut buf = [0u8; 1232];
    let mut decoded = 0;
    for _ in 0..100_000 {
        // Half full-size (valid non-last fragments), half any length.
        let len = if rng.below(2) == 0 {
            MAX_DATAGRAM
        } else {
            rng.below(buf.len() + 1)
        };
        for b in &mut buf[..len] {
            *b = rng.next() as u8;
        }
        // 90 %: a plausible header (mostly valid fields, small frame ids) to reach `push`.
        if len >= HEADER_LEN && rng.below(10) != 0 {
            let count = 1 + rng.below(6) as u16;
            buf[0] = 1;
            buf[1] = rng.below(8) as u8;
            buf[2..4].copy_from_slice(&(rng.below(5) as u16).to_le_bytes());
            buf[8..12].copy_from_slice(&(rng.below(16) as u32).to_le_bytes());
            buf[12..14].copy_from_slice(&(rng.below(usize::from(count) + 1) as u16).to_le_bytes());
            buf[14..16].copy_from_slice(&count.to_le_bytes());
        }
        if let Ok((h, payload)) = PacketHeader::decode(&buf[..len]) {
            decoded += 1;
            let _ = r.push(&h, payload);
        }
    }
    assert!(decoded > 10_000, "only {decoded} datagrams decoded");
    assert!(r.stats().completed > 0);
}

#[test]
fn perf_one_mib_frame() {
    let message = vec![0x5Au8; 1 << 20];
    let mut p = Packetizer::new(Channel::Video);
    let mut r = video();
    let mut buf = [0u8; MAX_DATAGRAM];
    let rounds = 1000u32;
    let start = Instant::now();
    for frame in 0..rounds {
        let mut fragments = p
            .packetize(FrameId::new(frame), Flags::empty(), &message)
            .unwrap();
        let mut complete = false;
        while let Some(len) = fragments.next_into(&mut buf) {
            let (h, body) = PacketHeader::decode(&buf[..len]).unwrap();
            complete |= matches!(r.push(&h, body), Push::Complete(_));
        }
        assert!(complete);
    }
    let per_frame = start.elapsed() / rounds;
    eprintln!(
        "packetize + reassemble 1 MiB: {} us per frame",
        per_frame.as_micros()
    );
}
