use std::time::Duration;

use mixlink_net::{RxLane, Transport, TransportConfig};
use mixlink_proto::crypto::{Opener, Role, SessionKeys};
use mixlink_proto::header::PacketHeader;
use mixlink_proto::reassembler::{Push, Reassembler, ReassemblerConfig};

use super::*;

pub(crate) fn keys() -> SessionKeys {
    SessionKeys {
        c2h: [1; 16],
        h2c: [2; 16],
    }
}

struct Pair {
    host: Transport,
    _client: Transport,
    rx: RxLane,
    opener: Opener,
    sender: VideoSender,
}

fn pair(video_slots: Option<usize>) -> Pair {
    let mut config = TransportConfig::loopback();
    if let Some(slots) = video_slots {
        config.lanes.set(Channel::Video, slots);
    }
    let (host, mut host_lanes) = Transport::start(config).unwrap();
    let (client, mut client_lanes) = Transport::start(TransportConfig::loopback()).unwrap();
    let sender = VideoSender::new(
        host_lanes.take_tx(Channel::Video).unwrap(),
        &keys(),
        client.local_addr(),
    );
    Pair {
        host,
        rx: client_lanes.take_rx(Channel::Video).unwrap(),
        _client: client,
        opener: keys().opener(Role::Client, Channel::Video),
        sender,
    }
}

/// Next complete video message as (frame id, keyframe flag, bytes).
fn receive(p: &mut Pair, reassembler: &mut Reassembler) -> Option<(u32, bool, Vec<u8>)> {
    while let Some(mut dg) = p.rx.recv_timeout(Duration::from_millis(500)) {
        let plain = dg.open(&mut p.opener).unwrap();
        let (header, payload) = PacketHeader::decode(plain).unwrap();
        if let Push::Complete(msg) = reassembler.push(&header, payload) {
            return Some((
                msg.frame.get(),
                msg.flags.contains(Flags::KEYFRAME),
                msg.data.to_vec(),
            ));
        }
    }
    None
}

fn au(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i % 253) as u8).collect()
}

fn meta(keyframe: bool) -> FrameMeta {
    FrameMeta {
        keyframe,
        capture_us: 400,
        encode_us: 2700,
        send_us: 0,
    }
}

#[test]
fn au_reassembles_with_meta() {
    let mut p = pair(None);
    let data = au(100 * 1024);
    assert_eq!(
        p.sender.send_au(&meta(true), &data),
        Delivery::Sent(data.len())
    );
    let mut reassembler = Reassembler::new(ReassemblerConfig::for_channel(Channel::Video));
    let (_, keyframe, msg) = receive(&mut p, &mut reassembler).unwrap();
    assert!(keyframe);
    let (got, body) = FrameMeta::split(&msg).unwrap();
    assert_eq!((got.capture_us, got.encode_us), (400, 2700));
    assert_eq!(body, data.as_slice());
}

#[test]
fn keyframe_flag_only_on_idr() {
    let mut p = pair(None);
    p.sender.send_au(&meta(false), &au(3000));
    let mut reassembler = Reassembler::new(ReassemblerConfig::for_channel(Channel::Video));
    let (_, keyframe, _) = receive(&mut p, &mut reassembler).unwrap();
    assert!(!keyframe);
}

#[test]
fn frame_ids_increase() {
    let mut p = pair(None);
    let mut reassembler = Reassembler::new(ReassemblerConfig::for_channel(Channel::Video));
    p.sender.send_au(&meta(true), &au(5000));
    let (first, _, _) = receive(&mut p, &mut reassembler).unwrap();
    p.sender.send_au(&meta(false), &au(5000));
    let (second, _, _) = receive(&mut p, &mut reassembler).unwrap();
    assert_eq!(second, first.wrapping_add(1));
}

#[test]
fn send_us_reports_the_previous_frame() {
    let mut p = pair(None);
    let mut reassembler = Reassembler::new(ReassemblerConfig::for_channel(Channel::Video));
    p.sender.send_au(&meta(true), &au(50_000));
    let (_, _, msg) = receive(&mut p, &mut reassembler).unwrap();
    assert_eq!(
        FrameMeta::split(&msg).unwrap().0.send_us,
        0,
        "no frame before"
    );
    p.sender.send_au(&meta(false), &au(10));
    let (_, _, msg) = receive(&mut p, &mut reassembler).unwrap();
    assert!(FrameMeta::split(&msg).unwrap().0.send_us > 0);
}

#[test]
fn full_lane_drops_rest_of_au() {
    let mut p = pair(Some(8));
    // Nobody drains the lane: the 9th fragment finds it full.
    p.host.shutdown();
    let data = au(100 * 1024);
    assert_eq!(
        p.sender.send_au(&meta(true), &data),
        Delivery::Dropped(data.len())
    );
    assert_eq!(
        p.sender.send_au(&meta(false), &data),
        Delivery::Dropped(data.len())
    );
}

#[test]
fn oversize_au_is_dropped() {
    let mut p = pair(None);
    let data = au(VIDEO_MESSAGE_MAX);
    assert_eq!(
        p.sender.send_au(&meta(true), &data),
        Delivery::Dropped(data.len())
    );
}
