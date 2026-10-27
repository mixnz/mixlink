use mixlink_proto::header::MAX_DATAGRAM;
use mixlink_proto::packetizer::Packetizer;
use mixlink_proto::video::FRAME_META_LEN;

use super::*;

/// The datagrams of one video message (meta + `au_len` bytes), as the host sends them.
fn datagrams(p: &mut Packetizer, frame: u32, keyframe: bool, au_len: usize) -> Vec<Vec<u8>> {
    let meta = FrameMeta {
        keyframe,
        capture_us: 400,
        encode_us: 2700,
        send_us: 30,
    };
    let mut msg = vec![0u8; FRAME_META_LEN];
    meta.encode(msg.first_chunk_mut().unwrap());
    msg.extend((0..au_len).map(|i| i as u8));
    let flags = if keyframe {
        Flags::KEYFRAME
    } else {
        Flags::empty()
    };
    let mut fragments = p.packetize(FrameId::new(frame), flags, &msg).unwrap();
    let mut out = Vec::new();
    let mut buf = [0u8; MAX_DATAGRAM];
    while let Some(len) = fragments.next_into(&mut buf) {
        out.push(buf[..len].to_vec());
    }
    out
}

/// Feeds every datagram; returns (au length, first, last) of each completed frame.
fn feed(rx: &mut VideoReceiver, dgs: &[Vec<u8>], t0: u64) -> Vec<(usize, u64, u64)> {
    let mut out = Vec::new();
    for (i, dg) in dgs.iter().enumerate() {
        if let Some(v) = rx.push(dg, Ticks::from_raw(t0 + i as u64)) {
            assert_eq!(v.meta.encode_us, 2700);
            out.push((v.au.len(), v.first.raw(), v.last.raw()));
        }
    }
    out
}

#[test]
fn frame_arrives_with_meta() {
    let mut p = Packetizer::new(Channel::Video);
    let mut rx = VideoReceiver::new();
    let dgs = datagrams(&mut p, 0, true, 5000);
    assert_eq!(dgs.len(), 5);
    assert_eq!(feed(&mut rx, &dgs, 100), vec![(5000, 100, 104)]);
    assert!(!rx.needs_keyframe());
    let next = datagrams(&mut p, 1, false, 10);
    assert_eq!(feed(&mut rx, &next, 200), vec![(10, 200, 200)]);
    assert_eq!(rx.counts().frames, 2);
}

#[test]
fn lost_fragment_requests_keyframe() {
    let mut p = Packetizer::new(Channel::Video);
    let mut rx = VideoReceiver::new();
    feed(&mut rx, &datagrams(&mut p, 0, true, 100), 0);
    let mut lossy = datagrams(&mut p, 1, false, 3000);
    lossy.remove(1);
    assert!(feed(&mut rx, &lossy, 10).is_empty());
    // A newer frame completes: frame 1 is given up.
    feed(&mut rx, &datagrams(&mut p, 2, false, 100), 20);
    assert!(rx.needs_keyframe());
    assert!(rx.counts().lost >= 1);
}

#[test]
fn non_key_dropped_until_keyframe() {
    let mut p = Packetizer::new(Channel::Video);
    let mut rx = VideoReceiver::new();
    feed(&mut rx, &datagrams(&mut p, 0, true, 100), 0);
    // Frame 1 never arrives at all: frame 2 is a skip.
    assert!(feed(&mut rx, &datagrams(&mut p, 2, false, 100), 10).is_empty());
    assert!(feed(&mut rx, &datagrams(&mut p, 3, false, 100), 20).is_empty());
    assert!(rx.needs_keyframe());
    assert_eq!(rx.counts().skipped, 2);
    assert_eq!(feed(&mut rx, &datagrams(&mut p, 4, true, 100), 30).len(), 1);
    assert!(!rx.needs_keyframe());
    assert_eq!(rx.last_good(), FrameId::new(4));
}

#[test]
fn nothing_received_asks_nothing() {
    let rx = VideoReceiver::new();
    assert!(!rx.needs_keyframe());
}

#[test]
fn decoder_failure_waits_for_keyframe() {
    let mut p = Packetizer::new(Channel::Video);
    let mut rx = VideoReceiver::new();
    feed(&mut rx, &datagrams(&mut p, 0, true, 100), 0);
    rx.lost_reference();
    assert!(rx.needs_keyframe());
    assert!(feed(&mut rx, &datagrams(&mut p, 1, false, 100), 10).is_empty());
}
