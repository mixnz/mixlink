use std::thread;

use mixlink_core::ids::FrameId;
use mixlink_input::client::state::InputState;
use mixlink_input::hid::keyboard::KeyboardState;
use mixlink_input::hid::report::{Report, Target};
use mixlink_input::host::abs_map::AbsMap;
use mixlink_net::{Transport, TransportConfig, TxLane};
use mixlink_proto::crypto::Sealer;
use mixlink_proto::header::Flags;
use mixlink_proto::input::{INPUT_LEN, encode};
use mixlink_proto::packetizer::Packetizer;

use super::*;
use crate::stream::video_send::tests::keys;

#[derive(Default)]
struct FakeSink {
    reports: Vec<Report>,
}

impl ReportSink for FakeSink {
    type Error = String;

    fn feed(&mut self, report: &Report) -> Result<(), String> {
        self.reports.push(*report);
        Ok(())
    }
}

struct Pair {
    _host: Transport,
    client: Transport,
    client_tx: TxLane,
    sealer: Sealer,
    packetizer: Packetizer,
    frame: FrameId,
    host_addr: SocketAddrV4,
    rx: RxLane,
    recv: InputRecv,
    state: InputState,
}

fn pair() -> Pair {
    let (host, mut host_lanes) = Transport::start(TransportConfig::loopback()).unwrap();
    let (client, mut client_lanes) = Transport::start(TransportConfig::loopback()).unwrap();
    let recv = InputRecv::new(
        &keys(),
        client.local_addr(),
        AbsMap::new((1920, 1080), None),
    );
    Pair {
        rx: host_lanes.take_rx(Channel::Input).unwrap(),
        host_addr: host.local_addr(),
        _host: host,
        client_tx: client_lanes.take_tx(Channel::Input).unwrap(),
        client,
        sealer: keys().sealer(Role::Client, Channel::Input),
        packetizer: Packetizer::new(Channel::Input),
        frame: FrameId::new(0),
        recv,
        state: InputState::new(),
    }
}

fn send(p: &mut Pair) {
    let packet = p.state.packet();
    let mut body = [0u8; INPUT_LEN];
    encode(&packet, &mut body);
    let mut fragments = p
        .packetizer
        .packetize(p.frame, Flags::empty(), &body)
        .unwrap();
    p.frame = p.frame.next();
    let mut slot = p.client_tx.try_reserve().unwrap();
    let len = fragments.next_into(slot.datagram()).unwrap();
    slot.seal_commit(&mut p.sealer, len, p.host_addr).unwrap();
}

fn keyboard(usages: &[u8]) -> Report {
    let mut k = KeyboardState::default();
    for &u in usages {
        k.set(u, true);
    }
    k.report()
}

/// Polls until a report arrives or 2 s pass.
fn poll_until_report(p: &mut Pair, sink: &mut FakeSink, now: Instant) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while sink.reports.is_empty() && Instant::now() < deadline {
        p.recv.poll(&mut p.rx, sink, Duration::from_millis(20), now);
    }
}

#[test]
fn key_reaches_sink() {
    let mut p = pair();
    let mut sink = FakeSink::default();
    p.state.key(0x04, true);
    send(&mut p);
    poll_until_report(&mut p, &mut sink, Instant::now());
    assert_eq!(sink.reports, vec![keyboard(&[0x04])]);
}

#[test]
fn other_peer_ignored() {
    let (host, mut host_lanes) = Transport::start(TransportConfig::loopback()).unwrap();
    let mut p = pair();
    // The receiver expects a peer that is not `p.client`.
    p.recv = InputRecv::new(&keys(), host.local_addr(), AbsMap::new((1920, 1080), None));
    p.rx = host_lanes.take_rx(Channel::Input).unwrap();
    p.host_addr = host.local_addr();
    let mut sink = FakeSink::default();
    p.state.key(0x04, true);
    send(&mut p);
    thread::sleep(Duration::from_millis(50));
    p.recv.poll(
        &mut p.rx,
        &mut sink,
        Duration::from_millis(50),
        Instant::now(),
    );
    assert!(sink.reports.is_empty());
    drop(p.client);
}

#[test]
fn silence_releases_held_keys() {
    let mut p = pair();
    let mut sink = FakeSink::default();
    let t0 = Instant::now();
    p.state.key(0x04, true);
    send(&mut p);
    poll_until_report(&mut p, &mut sink, t0);
    p.recv.poll(
        &mut p.rx,
        &mut sink,
        Duration::from_millis(1),
        t0 + Duration::from_millis(400),
    );
    assert_eq!(sink.reports.len(), 1);
    p.recv.poll(
        &mut p.rx,
        &mut sink,
        Duration::from_millis(1),
        t0 + Duration::from_millis(600),
    );
    assert_eq!(sink.reports[1], keyboard(&[]));
    assert_eq!(sink.reports[2].target(), Target::MouseRel);
    assert_eq!(sink.reports.len(), 3);
    p.recv.poll(
        &mut p.rx,
        &mut sink,
        Duration::from_millis(1),
        t0 + Duration::from_millis(900),
    );
    assert_eq!(sink.reports.len(), 3);
}

#[test]
fn finish_releases_all() {
    let mut p = pair();
    let mut sink = FakeSink::default();
    p.state.key(0xE1, true);
    send(&mut p);
    poll_until_report(&mut p, &mut sink, Instant::now());
    p.recv.finish(&mut sink);
    assert_eq!(sink.reports[1], keyboard(&[]));
}
