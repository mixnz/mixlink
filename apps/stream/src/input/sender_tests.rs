use std::time::{Duration, Instant};

use mixlink_input::client::state::InputState;
use mixlink_net::{Transport, TransportConfig};
use mixlink_proto::crypto::KEY_LEN;
use mixlink_proto::header::PacketHeader;
use mixlink_proto::input::decode;

use super::*;

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

fn keys() -> SessionKeys {
    SessionKeys {
        c2h: [3; KEY_LEN],
        h2c: [4; KEY_LEN],
    }
}

#[test]
fn schedule_repeats_at_4_and_16_ms() {
    let t0 = Instant::now();
    let mut s = Schedule::default();
    s.on_send(t0);
    assert_eq!(s.next(false), Some(t0 + ms(4)));
    assert!(!s.fire(t0 + ms(3), false));
    assert!(s.fire(t0 + ms(4), false));
    assert_eq!(s.next(false), Some(t0 + ms(16)));
    assert!(s.fire(t0 + ms(16), false));
    assert_eq!(s.next(false), None);
    assert!(!s.fire(t0 + ms(500), false));
}

#[test]
fn keepalive_only_while_held() {
    let t0 = Instant::now();
    let mut s = Schedule::default();
    s.on_send(t0);
    assert!(s.fire(t0 + ms(4), true));
    assert!(s.fire(t0 + ms(16), true));
    assert_eq!(s.next(true), Some(t0 + ms(116)));
    assert!(s.fire(t0 + ms(116), true));
    assert_eq!(s.next(true), Some(t0 + ms(216)));
    assert_eq!(s.next(false), None);
}

#[test]
fn new_packet_restarts_repeats() {
    let t0 = Instant::now();
    let mut s = Schedule::default();
    s.on_send(t0);
    assert!(s.fire(t0 + ms(4), false));
    s.on_send(t0 + ms(10));
    assert_eq!(s.next(false), Some(t0 + ms(14)));
}

#[test]
fn nothing_scheduled_before_first_send() {
    let s = Schedule::default();
    assert_eq!(s.next(true), None);
}

struct Pair {
    _host: Transport,
    _client: Transport,
    host_rx: mixlink_net::RxLane,
    tx: InputTx,
}

fn pair(drop: DropKnob) -> Pair {
    let (host, mut host_lanes) = Transport::start(TransportConfig::loopback()).unwrap();
    let (client, mut client_lanes) = Transport::start(TransportConfig::loopback()).unwrap();
    let tx = InputTx::new(
        client_lanes.take_tx(Channel::Input).unwrap(),
        &keys(),
        host.local_addr(),
        drop,
    );
    Pair {
        host_rx: host_lanes.take_rx(Channel::Input).unwrap(),
        _host: host,
        _client: client,
        tx,
    }
}

#[test]
fn sealed_packet_opens_on_host() {
    let mut p = pair(DropKnob::default());
    let mut state = InputState::new();
    state.key(0x04, true);
    let packet = state.packet();
    assert!(p.tx.send(&packet, false));
    assert!(p.tx.send(&packet, true));
    let mut opener = keys().opener(Role::Host, Channel::Input);
    for _ in 0..2 {
        let mut dg = p.host_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        let plain = dg.open(&mut opener).unwrap();
        let (_, body) = PacketHeader::decode(plain).unwrap();
        assert_eq!(decode(body).unwrap(), packet);
    }
    let counts = p.tx.counts();
    assert_eq!((counts.sent, counts.repeats), (2, 1));
}

#[test]
fn knob_drops_input() {
    let mut p = pair(DropKnob::parse("input=100").unwrap());
    let packet = InputState::new().packet();
    assert!(p.tx.send(&packet, false));
    assert!(p.host_rx.recv_timeout(Duration::from_millis(100)).is_none());
    assert_eq!(p.tx.counts().dropped, 1);
    assert_eq!(p.tx.counts().sent, 0);
}
