use std::thread;

use mixlink_core::ids::FrameId;
use mixlink_net::{Transport, TransportConfig, TxLane};
use mixlink_proto::crypto::Sealer;
use mixlink_proto::feedback::FEEDBACK_LEN;
use mixlink_proto::header::Flags;
use mixlink_proto::packetizer::Packetizer;

use super::*;
use crate::stream::video_send::tests::keys;

struct Pair {
    _host: Transport,
    _client: Transport,
    client_tx: TxLane,
    sealer: Sealer,
    packetizer: Packetizer,
    host_addr: SocketAddrV4,
    rx: FeedbackRx,
}

fn pair() -> Pair {
    let (host, mut host_lanes) = Transport::start(TransportConfig::loopback()).unwrap();
    let (client, mut client_lanes) = Transport::start(TransportConfig::loopback()).unwrap();
    let rx = FeedbackRx::new(
        host_lanes.take_rx(Channel::Feedback).unwrap(),
        &keys(),
        client.local_addr(),
    );
    Pair {
        host_addr: host.local_addr(),
        _host: host,
        client_tx: client_lanes.take_tx(Channel::Feedback).unwrap(),
        _client: client,
        sealer: keys().sealer(Role::Client, Channel::Feedback),
        packetizer: Packetizer::new(Channel::Feedback),
        rx,
    }
}

fn send_raw(p: &mut Pair, body: &[u8], sealed: bool) {
    let mut fragments = p
        .packetizer
        .packetize(FrameId::new(0), Flags::empty(), body)
        .unwrap();
    let mut slot = p.client_tx.try_reserve().unwrap();
    let len = fragments.next_into(slot.datagram()).unwrap();
    if sealed {
        slot.seal_commit(&mut p.sealer, len, p.host_addr).unwrap();
    } else {
        slot.commit(len, p.host_addr);
    }
}

fn send(p: &mut Pair, msg: Feedback) {
    let mut out = [0u8; FEEDBACK_LEN];
    let len = msg.encode(&mut out);
    send_raw(p, &out[..len], true);
}

/// Polls until something arrives (loopback delivery is fast but not instant).
fn poll_soon(p: &mut Pair, now: Instant) -> FeedbackPoll {
    for _ in 0..100 {
        let got = p.rx.poll(now);
        if got != FeedbackPoll::default() {
            return got;
        }
        thread::sleep(Duration::from_millis(5));
    }
    FeedbackPoll::default()
}

fn keyframe() -> Feedback {
    Feedback::KeyframeRequest {
        last_good: FrameId::new(9),
    }
}

#[test]
fn keyframe_request_forces_idr_once_per_50ms() {
    let mut p = pair();
    let t0 = Instant::now();
    send(&mut p, keyframe());
    assert!(poll_soon(&mut p, t0).force_idr);
    send(&mut p, keyframe());
    thread::sleep(Duration::from_millis(50));
    let soon = t0 + Duration::from_millis(10);
    assert!(!p.rx.poll(soon).force_idr, "inside the gap");
    assert!(
        p.rx.poll(t0 + IDR_GAP).force_idr,
        "kept until the gap is over"
    );
    assert!(
        !p.rx.poll(t0 + IDR_GAP * 3).force_idr,
        "one request, one IDR"
    );
}

#[test]
fn shape_request_flag() {
    let mut p = pair();
    send(&mut p, Feedback::CursorShapeRequest { shape_id: 5 });
    let got = poll_soon(&mut p, Instant::now());
    assert!(got.shape_request);
    assert!(!got.force_idr);
}

#[test]
fn unsealed_or_bad_kind_ignored() {
    let mut p = pair();
    let mut out = [0u8; FEEDBACK_LEN];
    keyframe().encode(&mut out);
    send_raw(&mut p, &out, false);
    send_raw(&mut p, &[9, 0, 0, 0, 0], true);
    thread::sleep(Duration::from_millis(100));
    assert_eq!(p.rx.poll(Instant::now()), FeedbackPoll::default());
}
