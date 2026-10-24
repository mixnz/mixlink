use std::net::{Ipv4Addr, SocketAddrV4, UdpSocket};
use std::time::{Duration, Instant};

use mixlink_core::clock::Ticks;
use mixlink_core::ids::{FrameId, Seq};
use mixlink_proto::channel::Channel;
use mixlink_proto::header::{Flags, HEADER_LEN, PacketHeader};
use mixlink_proto::packetizer::Packetizer;
use mixlink_proto::reassembler::{Push, Reassembler, ReassemblerConfig};

use super::*;
use crate::lane::{RxLane, SLOT_LEN, TxLane};
use crate::socket::SOCKET_BUFFER;

const WAIT: Duration = Duration::from_secs(1);

fn start(config: TransportConfig) -> (Transport, Lanes) {
    Transport::start(config).unwrap()
}

/// A datagram of `len` bytes (≥ 16) with a valid single-fragment header for `channel`.
fn datagram(channel: Channel, len: usize, tag: u8) -> Vec<u8> {
    let mut bytes = vec![tag; len];
    let header = PacketHeader {
        channel,
        flags: Flags::empty(),
        seq: Seq::new(0),
        frame: FrameId::new(0),
        frag_index: 0,
        frag_count: 1,
    };
    let mut head = [0u8; HEADER_LEN];
    header.encode(&mut head);
    bytes[..HEADER_LEN].copy_from_slice(&head);
    bytes
}

fn send(tx: &mut TxLane, bytes: &[u8], to: SocketAddrV4) {
    let deadline = Instant::now() + WAIT;
    loop {
        if let Some(mut slot) = tx.try_reserve() {
            slot.buf()[..bytes.len()].copy_from_slice(bytes);
            slot.commit(bytes.len(), to);
            return;
        }
        assert!(Instant::now() < deadline, "tx lane stayed full");
        std::thread::yield_now();
    }
}

fn recv(rx: &mut RxLane) -> Option<(Vec<u8>, SocketAddrV4, Ticks)> {
    rx.recv_timeout(WAIT)
        .map(|d| (d.data().to_vec(), d.peer(), d.at()))
}

fn all_lanes(lanes: &mut Lanes) -> Vec<(Channel, TxLane, RxLane)> {
    Channel::ALL
        .iter()
        .map(|&c| (c, lanes.take_tx(c).unwrap(), lanes.take_rx(c).unwrap()))
        .collect()
}

fn released_port() -> u16 {
    let probe = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    probe.local_addr().unwrap().port()
}

#[test]
fn every_channel_routes_to_its_lane() {
    let (a, mut a_lanes) = start(TransportConfig::loopback());
    let (b, mut b_lanes) = start(TransportConfig::loopback());
    let mut a_all = all_lanes(&mut a_lanes);
    let mut b_all = all_lanes(&mut b_lanes);
    for i in 0..a_all.len() {
        let channel = a_all[i].0;
        for len in [16, 17, 1216, SLOT_LEN] {
            let bytes = datagram(channel, len, channel.id());
            send(&mut a_all[i].1, &bytes, b.local_addr());
            let (got, peer, _) = recv(&mut b_all[i].2).unwrap();
            assert_eq!(got, bytes, "{channel:?} len {len}");
            assert_eq!(peer, a.local_addr());
        }
    }
    for (channel, _, rx) in &mut b_all {
        assert!(rx.try_recv().is_none(), "{channel:?} has extra datagrams");
    }
}

#[test]
fn megabyte_frame_reassembles() {
    let (_a, mut a_lanes) = start(TransportConfig::loopback());
    let (b, mut b_lanes) = start(TransportConfig::loopback());
    let mut tx = a_lanes.take_tx(Channel::Video).unwrap();
    let mut rx = b_lanes.take_rx(Channel::Video).unwrap();
    let frame: Vec<u8> = (0..1 << 20).map(|i: u32| (i % 251) as u8).collect();
    let mut packetizer = Packetizer::new(Channel::Video);
    let mut fragments = packetizer
        .packetize(FrameId::new(1), Flags::KEYFRAME, &frame)
        .unwrap();
    loop {
        let Some(mut slot) = tx.try_reserve() else {
            std::thread::yield_now();
            continue;
        };
        let Some(len) = fragments.next_into(slot.datagram()) else {
            break;
        };
        slot.commit(len, b.local_addr());
    }
    let mut reassembler = Reassembler::new(ReassemblerConfig::for_channel(Channel::Video));
    let mut complete = None;
    while complete.is_none() {
        let datagram = rx.recv_timeout(WAIT).expect("fragment missing");
        let (header, payload) = PacketHeader::decode(datagram.data()).unwrap();
        if let Push::Complete(message) = reassembler.push(&header, payload) {
            complete = Some(message.data.to_vec());
        }
    }
    assert!(complete == Some(frame), "reassembled frame differs");
}

#[test]
fn bad_datagrams_are_counted() {
    let (b, mut b_lanes) = start(TransportConfig::loopback());
    let mut b_all = all_lanes(&mut b_lanes);
    let raw = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let mut version_2 = datagram(Channel::Audio, 16, 0);
    version_2[0] = 2;
    let mut channel_0 = datagram(Channel::Audio, 16, 0);
    channel_0[1] = 0;
    let mut channel_7 = datagram(Channel::Audio, 16, 0);
    channel_7[1] = 7;
    let bad = [
        vec![1u8; 10],
        version_2,
        channel_0,
        channel_7,
        datagram(Channel::Audio, 1300, 0),
        datagram(Channel::Audio, 2000, 0),
    ];
    for bytes in &bad {
        raw.send_to(bytes, b.local_addr()).unwrap();
    }
    let good = datagram(Channel::Audio, 20, 5);
    raw.send_to(&good, b.local_addr()).unwrap();
    let audio = b_all.iter_mut().find(|l| l.0 == Channel::Audio).unwrap();
    assert_eq!(recv(&mut audio.2).unwrap().0, good);
    assert_eq!(b.stats().rx_bad, 6);
    for (channel, _, rx) in &mut b_all {
        assert!(rx.try_recv().is_none(), "{channel:?} got a bad datagram");
    }
}

#[test]
fn full_rx_lane_counts_and_continues() {
    let (_a, mut a_lanes) = start(TransportConfig::loopback());
    let mut config = TransportConfig::loopback();
    config.lanes.set(Channel::Audio, 4);
    let (b, mut b_lanes) = start(config);
    let mut tx = a_lanes.take_tx(Channel::Audio).unwrap();
    let mut rx = b_lanes.take_rx(Channel::Audio).unwrap();
    for i in 0..10 {
        send(&mut tx, &datagram(Channel::Audio, 20, i), b.local_addr());
    }
    let deadline = Instant::now() + WAIT;
    while b.stats().rx(Channel::Audio).full < 6 {
        assert!(Instant::now() < deadline, "{:?}", b.stats());
        std::thread::sleep(Duration::from_millis(1));
    }
    for i in 0..4 {
        assert_eq!(recv(&mut rx).unwrap().0, datagram(Channel::Audio, 20, i));
    }
    send(&mut tx, &datagram(Channel::Audio, 20, 99), b.local_addr());
    assert_eq!(recv(&mut rx).unwrap().0, datagram(Channel::Audio, 20, 99));
    assert_eq!(b.stats().rx(Channel::Audio).full, 6);
}

#[test]
fn input_overtakes_video_backlog() {
    let (_a, mut a_lanes) = start(TransportConfig::loopback());
    let (b, mut b_lanes) = start(TransportConfig::loopback());
    let mut video_tx = a_lanes.take_tx(Channel::Video).unwrap();
    let mut input_tx = a_lanes.take_tx(Channel::Input).unwrap();
    let mut video_rx = b_lanes.take_rx(Channel::Video).unwrap();
    let mut input_rx = b_lanes.take_rx(Channel::Input).unwrap();
    let video = datagram(Channel::Video, 1216, 1);
    for _ in 0..4000 {
        send(&mut video_tx, &video, b.local_addr());
    }
    send(
        &mut input_tx,
        &datagram(Channel::Input, 20, 2),
        b.local_addr(),
    );
    let mut last_video = Ticks::ZERO;
    let mut input_at = None;
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        while let Some(d) = video_rx.try_recv() {
            last_video = last_video.max(d.at());
        }
        if let Some(d) = input_rx.try_recv() {
            input_at = Some(d.at());
        }
        std::thread::yield_now();
    }
    let input_at = input_at.expect("input datagram lost");
    assert!(
        input_at < last_video,
        "input waited for the whole video backlog"
    );
}

#[test]
fn closed_port_keeps_recv_working() {
    let (a, mut a_lanes) = start(TransportConfig::loopback());
    let (_b, mut b_lanes) = start(TransportConfig::loopback());
    let mut a_tx = a_lanes.take_tx(Channel::Control).unwrap();
    let mut a_rx = a_lanes.take_rx(Channel::Control).unwrap();
    let mut b_tx = b_lanes.take_tx(Channel::Control).unwrap();
    let closed = SocketAddrV4::new(Ipv4Addr::LOCALHOST, released_port());
    send(&mut a_tx, &datagram(Channel::Control, 20, 1), closed);
    std::thread::sleep(Duration::from_millis(50));
    let bytes = datagram(Channel::Control, 20, 2);
    send(&mut b_tx, &bytes, a.local_addr());
    assert_eq!(recv(&mut a_rx).unwrap().0, bytes);
    assert_eq!(a.stats().recv_err, 0);
}

#[test]
fn shutdown_joins_quickly() {
    let (mut a, _lanes) = start(TransportConfig::loopback());
    let start_at = Instant::now();
    a.shutdown();
    assert!(start_at.elapsed() < Duration::from_millis(100));
    a.shutdown();
    let (b, _lanes) = start(TransportConfig::loopback());
    let start_at = Instant::now();
    drop(b);
    assert!(start_at.elapsed() < Duration::from_millis(100));
}

#[test]
fn lanes_survive_transport_drop() {
    let (a, mut lanes) = start(TransportConfig::loopback());
    let to = a.local_addr();
    let mut tx = lanes.take_tx(Channel::Input).unwrap();
    let mut rx = lanes.take_rx(Channel::Input).unwrap();
    drop(a);
    for _ in 0..1000 {
        if let Some(mut slot) = tx.try_reserve() {
            slot.buf()[..20].copy_from_slice(&datagram(Channel::Input, 20, 0));
            slot.commit(20, to);
        }
    }
    assert!(tx.try_reserve().is_none());
    assert!(rx.try_recv().is_none());
}

#[test]
fn stats_report_buffers() {
    let (a, _lanes) = start(TransportConfig::loopback());
    let stats = a.stats();
    assert!(stats.recv_buffer >= SOCKET_BUFFER, "{stats:?}");
    assert!(stats.send_buffer >= SOCKET_BUFFER, "{stats:?}");
}

#[test]
fn lanes_are_taken_once() {
    let (_a, mut lanes) = start(TransportConfig::loopback());
    assert!(lanes.take_tx(Channel::Video).is_some());
    assert!(lanes.take_tx(Channel::Video).is_none());
    assert!(lanes.take_rx(Channel::Video).is_some());
    assert!(lanes.take_rx(Channel::Video).is_none());
}
