//! Drives `HostControl` and a client `ControlLink` from one thread over two loopback transports.

use std::time::{Duration, Instant};

use mixlink_proto::channel::Channel;
use mixlink_proto::control::{Blob, DisplayInfo};
use mixlink_proto::header::WIRE_VERSION;

use super::*;
use crate::control_link::{Incoming, Wire};
use crate::keys::ClientIdentity;
use crate::lane::RxLane;
use crate::pairing::KnownHost;
use crate::pairing::client::{self as pair_client, PairContext};
use crate::session::client as session_client;
use crate::transport::{Transport, TransportConfig};

const HOST: DeviceId = DeviceId::from_bytes([0xaa; 16]);

struct Side {
    _transport: Transport,
    tx: TxLane,
    rx: RxLane,
    addr: SocketAddrV4,
}

fn side() -> Side {
    let (transport, mut lanes) = Transport::start(TransportConfig::loopback()).unwrap();
    let addr = transport.local_addr();
    Side {
        tx: lanes.take_tx(Channel::Control).unwrap(),
        rx: lanes.take_rx(Channel::Control).unwrap(),
        _transport: transport,
        addr,
    }
}

struct Harness {
    host: HostControl,
    host_side: Side,
    host_pub: PublicKey,
    t0: Instant,
}

use mixlink_proto::control::PublicKey;

fn harness(paired: Vec<PairedDevice>) -> Harness {
    let keys = StaticKeyPair::generate().unwrap();
    let host_pub = keys.public;
    Harness {
        host: HostControl::new(HostControlConfig {
            host: HOST,
            keys,
            paired,
        }),
        host_side: side(),
        host_pub,
        t0: Instant::now(),
    }
}

fn at(h: &Harness, secs: u64) -> Now {
    Now {
        mono: h.t0 + Duration::from_secs(secs),
        unix_s: secs,
    }
}

/// Feeds every datagram the host gets within 200 ms to `HostControl`.
fn pump(h: &mut Harness, secs: u64) -> Vec<EndpointEvent> {
    let now = at(h, secs);
    let mut events = Vec::new();
    while let Some(mut dg) = h.host_side.rx.recv_timeout(Duration::from_millis(200)) {
        events.extend(h.host.on_datagram(&mut h.host_side.tx, &mut dg, now));
    }
    events
}

/// The next complete control message the client gets.
fn client_recv(client: &mut Side, link: &mut ControlLink) -> Option<Incoming> {
    while let Some(mut dg) = client.rx.recv_timeout(Duration::from_millis(500)) {
        if let Some(incoming) = link.accept(&mut dg) {
            return Some(incoming);
        }
    }
    None
}

fn identity(byte: u8) -> ClientIdentity {
    ClientIdentity {
        id: DeviceId::from_bytes([byte; 16]),
        keys: StaticKeyPair::generate().unwrap(),
    }
}

fn paired(me: &ClientIdentity) -> PairedDevice {
    PairedDevice {
        device: me.id,
        name: "Laptop".to_owned(),
        key: me.keys.public,
        paired_at: 0,
    }
}

fn hello(me: &ClientIdentity) -> ControlMessage {
    ControlMessage::Hello {
        proto: WIRE_VERSION,
        device: me.id,
        name: "Laptop".to_owned(),
        displays: vec![DisplayInfo {
            width: 2560,
            height: 1440,
            refresh_mhz: 120_000,
            hdr: false,
            dpi: 96,
        }],
        stream_display: 0,
        codecs: vec![mixlink_proto::control::CodecId::H264],
    }
}

/// Runs the handshake for `me` from `client`; returns the client link with keys set.
fn handshake(
    h: &mut Harness,
    client: &mut Side,
    me: &ClientIdentity,
    secs: u64,
) -> (ControlLink, Vec<EndpointEvent>) {
    let known = KnownHost {
        host: HOST,
        name: "Desk".to_owned(),
        key: h.host_pub,
    };
    let mut link = ControlLink::new(h.host_side.addr);
    let (start, init) = session_client::init(me, &known, &hello(me)).unwrap();
    link.send(&mut client.tx, &init).unwrap();
    let events = pump(h, secs);
    let reply = client_recv(client, &mut link).unwrap();
    let (keys, _, _) = session_client::finish(start, &reply.msg).unwrap();
    link.set_keys(&keys, Role::Client);
    (link, events)
}

fn ping(
    h: &mut Harness,
    client: &mut Side,
    link: &mut ControlLink,
    n: u32,
    secs: u64,
) -> Vec<EndpointEvent> {
    link.send_sealed(&mut client.tx, &ControlMessage::Ping { n })
        .unwrap();
    let events = pump(h, secs);
    let pong = client_recv(client, link).unwrap();
    assert_eq!(
        pong,
        Incoming {
            msg: ControlMessage::Pong { n },
            sealed: true
        }
    );
    events
}

#[test]
fn commit_only_after_sealed() {
    let me = identity(1);
    let mut h = harness(vec![paired(&me)]);
    let mut client = side();
    let (mut link, _) = handshake(&mut h, &mut client, &me, 0);
    let events = ping(&mut h, &mut client, &mut link, 0, 1);
    assert!(matches!(
        events.as_slice(),
        [EndpointEvent::SessionStarted { device, peer, .. }] if *device == me.id && *peer == client.addr
    ));
    assert!(ping(&mut h, &mut client, &mut link, 1, 2).is_empty());
}

#[test]
fn expire_after_10s_without_datagram() {
    let me = identity(1);
    let mut h = harness(vec![paired(&me)]);
    let mut client = side();
    let (mut link, _) = handshake(&mut h, &mut client, &me, 0);
    ping(&mut h, &mut client, &mut link, 0, 1);
    assert!(h.host.tick(at(&h, 10)).is_empty());
    assert!(matches!(
        h.host.tick(at(&h, 11)).as_slice(),
        [EndpointEvent::SessionEnded { device, .. }] if *device == me.id
    ));
}

#[test]
fn bye_ends_session() {
    let me = identity(1);
    let mut h = harness(vec![paired(&me)]);
    let mut client = side();
    let (mut link, _) = handshake(&mut h, &mut client, &me, 0);
    ping(&mut h, &mut client, &mut link, 0, 1);
    let bye = ControlMessage::Bye {
        reason: mixlink_proto::control::ByeReason::UserClosed,
    };
    link.send_sealed(&mut client.tx, &bye).unwrap();
    assert!(matches!(
        pump(&mut h, 2).as_slice(),
        [EndpointEvent::SessionEnded { .. }]
    ));
}

#[test]
fn new_session_init_replaces_old_session() {
    let me = identity(1);
    let mut h = harness(vec![paired(&me)]);
    let mut client = side();
    let (mut link, _) = handshake(&mut h, &mut client, &me, 0);
    ping(&mut h, &mut client, &mut link, 0, 1);
    // The client restarts on a new socket: no Busy, the old session ends, the new one starts.
    let mut again = side();
    let (mut link2, ended) = handshake(&mut h, &mut again, &me, 2);
    assert!(
        matches!(ended.as_slice(), [EndpointEvent::SessionEnded { device, .. }] if *device == me.id)
    );
    let events = ping(&mut h, &mut again, &mut link2, 0, 3);
    assert!(matches!(
        events.as_slice(),
        [EndpointEvent::SessionStarted { peer, .. }] if *peer == again.addr
    ));
    assert!(
        h.host.tick(at(&h, 12)).is_empty(),
        "old session already ended"
    );
}

#[test]
fn other_device_busy_while_streaming() {
    let me = identity(1);
    let other = identity(2);
    let mut h = harness(vec![paired(&me), paired(&other)]);
    let mut client = side();
    let (mut link, _) = handshake(&mut h, &mut client, &me, 0);
    ping(&mut h, &mut client, &mut link, 0, 1);
    let known = KnownHost {
        host: HOST,
        name: "Desk".to_owned(),
        key: h.host_pub,
    };
    let mut second = side();
    let mut link2 = ControlLink::new(h.host_side.addr);
    let (_, init) = session_client::init(&other, &known, &hello(&other)).unwrap();
    link2.send(&mut second.tx, &init).unwrap();
    pump(&mut h, 2);
    let reply = client_recv(&mut second, &mut link2).unwrap();
    assert_eq!(
        reply.msg,
        ControlMessage::Reject {
            reason: RejectReason::Busy
        }
    );
}

#[test]
fn removed_device_session_ends() {
    let me = identity(1);
    let mut h = harness(vec![paired(&me)]);
    let mut client = side();
    let (mut link, _) = handshake(&mut h, &mut client, &me, 0);
    ping(&mut h, &mut client, &mut link, 0, 1);
    let events = h.host.set_paired(Vec::new());
    assert!(matches!(
        events.as_slice(),
        [EndpointEvent::SessionEnded { .. }]
    ));
    assert!(h.host.paired().is_empty());
}

#[test]
fn pairing_through_endpoint() {
    let me = identity(1);
    let mut h = harness(Vec::new());
    let mut client = side();
    let mut link = ControlLink::new(h.host_side.addr);
    let ctx = PairContext {
        me: me.clone(),
        name: "Laptop".to_owned(),
        host: HOST,
        host_name: "Desk".to_owned(),
    };
    link.send(&mut client.tx, &ctx.request()).unwrap();
    pump(&mut h, 0);
    assert!(matches!(
        client_recv(&mut client, &mut link).unwrap().msg,
        ControlMessage::PairPrompt { .. }
    ));
    let pin: u32 = h.host.prompt(at(&h, 0)).unwrap().pin.parse().unwrap();
    let (attempt, start) = pair_client::start(&ctx, pin);
    link.send(&mut client.tx, &start).unwrap();
    pump(&mut h, 1);
    let reply = client_recv(&mut client, &mut link).unwrap().msg;
    let (confirm, known) = pair_client::finish(&ctx, attempt, &reply).unwrap();
    assert_eq!(known.key, h.host_pub);
    link.send(&mut client.tx, &confirm).unwrap();
    let events = pump(&mut h, 2);
    assert!(matches!(events.as_slice(), [EndpointEvent::Paired(device)] if device.device == me.id));
    assert_eq!(
        client_recv(&mut client, &mut link).unwrap().msg,
        ControlMessage::PairDone
    );
    assert_eq!(h.host.paired().len(), 1);
}

/// Opens a prompt for `me`; returns the client's link to the host.
fn open_prompt(h: &mut Harness, client: &mut Side, me: &ClientIdentity) -> ControlLink {
    let mut link = ControlLink::new(h.host_side.addr);
    let ctx = PairContext {
        me: me.clone(),
        name: "Laptop".to_owned(),
        host: HOST,
        host_name: "Desk".to_owned(),
    };
    link.send(&mut client.tx, &ctx.request()).unwrap();
    pump(h, 0);
    assert!(matches!(
        client_recv(client, &mut link).unwrap().msg,
        ControlMessage::PairPrompt { .. }
    ));
    link
}

#[test]
fn cancel_tells_waiting_client() {
    let me = identity(1);
    let mut h = harness(Vec::new());
    let mut client = side();
    let mut link = open_prompt(&mut h, &mut client, &me);
    h.host.cancel_prompt(&mut h.host_side.tx);
    assert!(h.host.prompt(at(&h, 1)).is_none());
    std::thread::sleep(Duration::from_millis(100));
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let mut wire = Wire {
        tx: &mut client.tx,
        rx: &mut client.rx,
        cancel: &cancel,
    };
    assert!(pair_client::closed_by_host(&mut link, &mut wire));
    // Nothing is sent once no prompt is open.
    h.host.cancel_prompt(&mut h.host_side.tx);
    std::thread::sleep(Duration::from_millis(100));
    assert!(client_recv(&mut client, &mut link).is_none());
}

#[test]
fn prompt_peer_outlives_idle_time() {
    let me = identity(1);
    let mut h = harness(Vec::new());
    let mut client = side();
    let mut link = open_prompt(&mut h, &mut client, &me);
    assert!(h.host.tick(at(&h, 90)).is_empty());
    // A reply from a fresh host link would restart frame numbers and be dropped as late.
    link.send(
        &mut client.tx,
        &ControlMessage::PairStart {
            device: me.id,
            spake: Blob(vec![0; 33]),
        },
    )
    .unwrap();
    pump(&mut h, 91);
    assert!(client_recv(&mut client, &mut link).is_some());
}
