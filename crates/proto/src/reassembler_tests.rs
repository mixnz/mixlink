use std::time::Instant;

use super::*;
use crate::header::{HEADER_LEN, MAX_DATAGRAM};
use crate::packetizer::Packetizer;

/// Deterministic xorshift64; no `rand` dependency.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            items.swap(i, self.below(i + 1));
        }
    }
}

type Datagram = (PacketHeader, Vec<u8>);

fn datagrams(p: &mut Packetizer, frame: u32, flags: Flags, message: &[u8]) -> Vec<Datagram> {
    let mut fragments = p.packetize(FrameId::new(frame), flags, message).unwrap();
    let mut buf = [0u8; MAX_DATAGRAM];
    let mut out = Vec::new();
    while let Some(len) = fragments.next_into(&mut buf) {
        let (h, body) = PacketHeader::decode(&buf[..len]).unwrap();
        out.push((h, body.to_vec()));
    }
    out
}

/// Outcome of one push, owned so the reassembler can be pushed again.
#[derive(Debug, PartialEq)]
enum Out {
    Complete(u32, Flags, Vec<u8>),
    Pending,
    Dropped(DropReason),
}

fn push(r: &mut Reassembler, d: &Datagram) -> Out {
    match r.push(&d.0, &d.1) {
        Push::Complete(m) => Out::Complete(m.frame.get(), m.flags, m.data.to_vec()),
        Push::Pending => Out::Pending,
        Push::Dropped(reason) => Out::Dropped(reason),
    }
}

fn completes(r: &mut Reassembler, ds: &[Datagram]) -> Vec<Out> {
    ds.iter()
        .map(|d| push(r, d))
        .filter(|o| matches!(o, Out::Complete(..)))
        .collect()
}

fn video() -> Reassembler {
    Reassembler::new(ReassemblerConfig::for_channel(Channel::Video))
}

fn two_slots() -> Reassembler {
    Reassembler::new(ReassemblerConfig {
        slots: 2,
        max_message_len: 65_536,
    })
}

#[test]
fn channel_defaults_match_spec() {
    let cfg = |c| ReassemblerConfig::for_channel(c);
    assert_eq!(
        (
            cfg(Channel::Control).slots,
            cfg(Channel::Control).max_message_len
        ),
        (2, 65_536)
    );
    assert_eq!(
        (
            cfg(Channel::Video).slots,
            cfg(Channel::Video).max_message_len
        ),
        (4, 4_194_304)
    );
    assert_eq!(cfg(Channel::Audio).max_message_len, 1200);
    assert_eq!(cfg(Channel::Cursor).max_message_len, 270_336);
    assert_eq!(cfg(Channel::Input).max_message_len, 1200);
    assert_eq!(cfg(Channel::Feedback).max_message_len, 1200);
}

#[test]
fn round_trip_sizes_shuffled() {
    let mut rng = Rng(0x1234_5678);
    let mut p = Packetizer::new(Channel::Video);
    let mut r = video();
    for (frame, size) in [0usize, 1, 1200, 1201, 1 << 20].into_iter().enumerate() {
        let message: Vec<u8> = (0..size).map(|_| rng.next() as u8).collect();
        let mut ds = datagrams(&mut p, frame as u32, Flags::KEYFRAME, &message);
        rng.shuffle(&mut ds);
        let done = completes(&mut r, &ds);
        assert_eq!(
            done,
            [Out::Complete(frame as u32, Flags::KEYFRAME, message)]
        );
    }
    assert_eq!(r.stats().completed, 5);
}

#[test]
fn single_packet_is_not_copied() {
    let mut p = Packetizer::new(Channel::Audio);
    let ds = datagrams(&mut p, 1, Flags::empty(), &[1, 2, 3]);
    let mut r = Reassembler::new(ReassemblerConfig::for_channel(Channel::Audio));
    let Push::Complete(m) = r.push(&ds[0].0, &ds[0].1) else {
        panic!("expected complete");
    };
    assert_eq!(m.data.as_ptr(), ds[0].1.as_ptr());
}

#[test]
fn empty_message_completes_empty() {
    let mut p = Packetizer::new(Channel::Control);
    let ds = datagrams(&mut p, 4, Flags::empty(), &[]);
    assert_eq!(
        push(&mut two_slots(), &ds[0]),
        Out::Complete(4, Flags::empty(), vec![])
    );
}

#[test]
fn interleaved_frames_both_complete() {
    let mut p = Packetizer::new(Channel::Video);
    let a = datagrams(&mut p, 1, Flags::empty(), &[1; 3000]);
    let b = datagrams(&mut p, 2, Flags::empty(), &[2; 3000]);
    let mixed = [&a[0], &b[0], &a[1], &b[1], &b[2], &a[2]];
    let mut r = video();
    let outs: Vec<Out> = mixed.iter().map(|d| push(&mut r, d)).collect();
    assert_eq!(outs[4], Out::Complete(2, Flags::empty(), vec![2; 3000]));
    assert_eq!(outs[5], Out::Dropped(DropReason::Late));
}

#[test]
fn interleaved_frames_complete_in_order() {
    let mut p = Packetizer::new(Channel::Video);
    let a = datagrams(&mut p, 1, Flags::empty(), &[1; 3000]);
    let b = datagrams(&mut p, 2, Flags::empty(), &[2; 3000]);
    let mixed = [&a[0], &b[0], &a[1], &b[1], &a[2], &b[2]];
    let mut r = video();
    let outs: Vec<Out> = mixed.iter().map(|d| push(&mut r, d)).collect();
    assert_eq!(outs[4], Out::Complete(1, Flags::empty(), vec![1; 3000]));
    assert_eq!(outs[5], Out::Complete(2, Flags::empty(), vec![2; 3000]));
    assert_eq!(r.stats().evicted, 0);
}

#[test]
fn lost_fragment_is_evicted_when_slots_run_out() {
    let mut p = Packetizer::new(Channel::Video);
    let f1 = datagrams(&mut p, 1, Flags::empty(), &[1; 3000]);
    let f2 = datagrams(&mut p, 2, Flags::empty(), &[2; 3000]);
    let f3 = datagrams(&mut p, 3, Flags::empty(), &[3; 3000]);
    let mut r = two_slots();
    assert!(completes(&mut r, &f1[..2]).is_empty());
    // Frame 2 partial too, so frame 3 needs a slot: frame 1 (oldest) is evicted.
    assert!(completes(&mut r, &f2[..2]).is_empty());
    assert!(completes(&mut r, &f3[..1]).is_empty());
    assert_eq!(r.stats().evicted, 1);
    assert_eq!(push(&mut r, &f1[2]), Out::Dropped(DropReason::Late));
    assert_eq!(completes(&mut r, &f2[2..]).len(), 1);
    assert_eq!(completes(&mut r, &f3[1..]).len(), 1);
}

#[test]
fn older_pending_frame_is_dropped_on_newer_complete() {
    let mut p = Packetizer::new(Channel::Video);
    let f1 = datagrams(&mut p, 1, Flags::empty(), &[1; 3000]);
    let f2 = datagrams(&mut p, 2, Flags::empty(), &[2; 3000]);
    let mut r = two_slots();
    completes(&mut r, &f1[..2]);
    assert_eq!(completes(&mut r, &f2).len(), 1);
    assert_eq!(r.stats().evicted, 1);
    assert_eq!(push(&mut r, &f1[2]), Out::Dropped(DropReason::Late));
    assert_eq!(r.stats().late, 1);
}

#[test]
fn duplicate_fragment_is_counted() {
    let mut p = Packetizer::new(Channel::Video);
    let f = datagrams(&mut p, 1, Flags::empty(), &[1; 3000]);
    let mut r = video();
    assert_eq!(push(&mut r, &f[0]), Out::Pending);
    assert_eq!(push(&mut r, &f[0]), Out::Dropped(DropReason::Duplicate));
    assert_eq!(r.stats().duplicate, 1);
    assert_eq!(completes(&mut r, &f[1..]).len(), 1);
}

#[test]
fn duplicate_after_complete_is_late() {
    let mut p = Packetizer::new(Channel::Video);
    let f = datagrams(&mut p, 1, Flags::empty(), &[1; 3000]);
    let mut r = video();
    assert_eq!(completes(&mut r, &f).len(), 1);
    assert_eq!(push(&mut r, &f[1]), Out::Dropped(DropReason::Late));
    let single = datagrams(&mut p, 2, Flags::empty(), &[5]);
    assert_eq!(completes(&mut r, &single).len(), 1);
    assert_eq!(push(&mut r, &single[0]), Out::Dropped(DropReason::Late));
    assert_eq!(r.stats().completed, 2);
}

#[test]
fn frame_id_wrap_is_not_late() {
    let mut p = Packetizer::new(Channel::Video);
    let last = datagrams(&mut p, u32::MAX, Flags::empty(), &[1; 3000]);
    let first = datagrams(&mut p, 0, Flags::empty(), &[2; 3000]);
    let mut r = video();
    assert_eq!(completes(&mut r, &last).len(), 1);
    assert_eq!(
        completes(&mut r, &first),
        [Out::Complete(0, Flags::empty(), vec![2; 3000])]
    );
}

#[test]
fn inconsistent_frag_count_is_malformed() {
    let mut p = Packetizer::new(Channel::Video);
    let f = datagrams(&mut p, 1, Flags::empty(), &[1; 3000]);
    let mut r = video();
    push(&mut r, &f[0]);
    let mut bad = f[1].clone();
    bad.0.frag_count = 4;
    assert_eq!(push(&mut r, &bad), Out::Dropped(DropReason::Malformed));
    let mut bad_flags = f[1].clone();
    bad_flags.0.flags = Flags::KEYFRAME;
    assert_eq!(
        push(&mut r, &bad_flags),
        Out::Dropped(DropReason::Malformed)
    );
    assert_eq!(r.stats().malformed, 2);
}

#[test]
fn message_over_limit_is_malformed() {
    let mut p = Packetizer::new(Channel::Audio);
    let f = datagrams(&mut p, 1, Flags::empty(), &[1; 1201]);
    let mut r = Reassembler::new(ReassemblerConfig::for_channel(Channel::Audio));
    assert_eq!(push(&mut r, &f[0]), Out::Dropped(DropReason::Malformed));
    assert_eq!(push(&mut r, &f[1]), Out::Dropped(DropReason::Malformed));
}

#[test]
fn message_at_limit_with_short_last_fragment_completes() {
    let mut p = Packetizer::new(Channel::Control);
    let f = datagrams(&mut p, 1, Flags::empty(), &[7; 2000]);
    let mut r = Reassembler::new(ReassemblerConfig {
        slots: 1,
        max_message_len: 2000,
    });
    assert_eq!(
        completes(&mut r, &f),
        [Out::Complete(1, Flags::empty(), vec![7; 2000])]
    );
}

#[test]
fn seq_gaps_counted() {
    let mut p = Packetizer::new(Channel::Input);
    let ds: Vec<Datagram> = (0..5)
        .flat_map(|i| datagrams(&mut p, i, Flags::empty(), &[i as u8]))
        .collect();
    let mut r = Reassembler::new(ReassemblerConfig::for_channel(Channel::Input));
    for i in [0, 1, 4] {
        push(&mut r, &ds[i]);
    }
    assert_eq!(r.stats().seq_gaps, 2);
    push(&mut r, &ds[2]);
    assert_eq!(r.stats().seq_gaps, 2);
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

#[test]
fn header_not_from_decode_is_malformed() {
    let mut r = video();
    let mut h = datagrams(
        &mut Packetizer::new(Channel::Video),
        1,
        Flags::empty(),
        &[1],
    )
    .remove(0);
    h.0.frag_count = 0;
    assert_eq!(push(&mut r, &h), Out::Dropped(DropReason::Malformed));
    h.0.frag_count = 2;
    h.0.frag_index = 2;
    assert_eq!(push(&mut r, &h), Out::Dropped(DropReason::Malformed));
}

#[test]
fn frame_older_than_every_slot_does_not_evict_newer() {
    let mut p = Packetizer::new(Channel::Video);
    let f1 = datagrams(&mut p, 1, Flags::empty(), &[1; 3000]);
    let f2 = datagrams(&mut p, 2, Flags::empty(), &[2; 3000]);
    let f3 = datagrams(&mut p, 3, Flags::empty(), &[3; 3000]);
    let mut r = two_slots();
    completes(&mut r, &f2[..1]);
    completes(&mut r, &f3[..1]);
    // Frame 1 is reordered behind 2 and 3; it must not push frame 2 out.
    assert_eq!(push(&mut r, &f1[0]), Out::Dropped(DropReason::Late));
    assert_eq!(r.stats().evicted, 0);
    assert_eq!(completes(&mut r, &f2[1..]).len(), 1);
    assert_eq!(completes(&mut r, &f3[1..]).len(), 1);
}
