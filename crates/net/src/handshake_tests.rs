use std::net::SocketAddrV4;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use mixlink_core::ids::{DeviceId, FrameId, SessionId};
use mixlink_proto::channel::Channel;
use mixlink_proto::control::{self, ByeReason, ControlMessage, DisplayInfo, RejectReason};
use mixlink_proto::header::{Flags, PacketHeader, WIRE_VERSION};
use mixlink_proto::packetizer::Packetizer;
use mixlink_proto::reassembler::{Push, Reassembler, ReassemblerConfig};

use super::*;
use crate::lane::{RxLane, TxLane};
use crate::transport::{Transport, TransportConfig};

const INTERVAL: Duration = Duration::from_millis(50);
const GIVE_UP: Duration = Duration::from_secs(1);

fn mode() -> DisplayInfo {
    DisplayInfo {
        width: 2560,
        height: 1440,
        refresh_mhz: 120_000,
        hdr: false,
        dpi: 96,
    }
}

fn hello() -> ControlMessage {
    ControlMessage::Hello {
        proto: WIRE_VERSION,
        device: DeviceId::from_bytes([7; 16]),
        name: "test".into(),
        displays: vec![mode()],
        stream_display: 0,
    }
}

fn welcome() -> Vec<u8> {
    control::encode(&ControlMessage::Welcome {
        session: SessionId::new(5),
        mode: mode(),
    })
    .unwrap()
}

fn encoded(message: &ControlMessage) -> Vec<u8> {
    control::encode(message).unwrap()
}

/// Sends one control message (any bytes) to `to`.
fn send(tx: &mut TxLane, packetizer: &mut Packetizer, frame: u32, bytes: &[u8], to: SocketAddrV4) {
    let mut fragments = packetizer
        .packetize(FrameId::new(frame), Flags::empty(), bytes)
        .unwrap();
    loop {
        let mut slot = tx.try_reserve().unwrap();
        let Some(len) = fragments.next_into(slot.datagram()) else {
            break;
        };
        slot.commit(len, to);
    }
}

struct Client {
    _transport: Transport,
    addr: SocketAddrV4,
    tx: TxLane,
    rx: RxLane,
}

fn client() -> Client {
    let (transport, mut lanes) = Transport::start(TransportConfig::loopback()).unwrap();
    Client {
        addr: transport.local_addr(),
        tx: lanes.take_tx(Channel::Control).unwrap(),
        rx: lanes.take_rx(Channel::Control).unwrap(),
        _transport: transport,
    }
}

struct Host {
    addr: SocketAddrV4,
    stop: Arc<AtomicBool>,
    thread: JoinHandle<Vec<FrameId>>,
}

impl Host {
    fn hellos(self) -> Vec<FrameId> {
        self.stop.store(true, Ordering::Release);
        self.thread.join().unwrap()
    }
}

/// Test host: for the n-th `Hello` (0-based) `reply(n)` returns the payloads to send back.
fn host(mut reply: impl FnMut(usize) -> Vec<Vec<u8>> + Send + 'static) -> Host {
    let (transport, mut lanes) = Transport::start(TransportConfig::loopback()).unwrap();
    let addr = transport.local_addr();
    let stop = Arc::new(AtomicBool::new(false));
    let stop_host = Arc::clone(&stop);
    let mut tx = lanes.take_tx(Channel::Control).unwrap();
    let mut rx = lanes.take_rx(Channel::Control).unwrap();
    let thread = thread::spawn(move || {
        let _transport = transport;
        let mut reassembler = Reassembler::new(ReassemblerConfig::for_channel(Channel::Control));
        let mut packetizer = Packetizer::new(Channel::Control);
        let mut hellos = Vec::new();
        let mut frame = 0;
        while !stop_host.load(Ordering::Acquire) {
            let Some(datagram) = rx.recv_timeout(Duration::from_millis(10)) else {
                continue;
            };
            let peer = datagram.peer();
            let Ok((header, payload)) = PacketHeader::decode(datagram.data()) else {
                continue;
            };
            let Push::Complete(message) = reassembler.push(&header, payload) else {
                continue;
            };
            let id = message.frame;
            if let Ok(ControlMessage::Hello { .. }) = control::decode(message.data) {
                for bytes in reply(hellos.len()) {
                    frame += 1;
                    send(&mut tx, &mut packetizer, frame, &bytes, peer);
                }
                hellos.push(id);
            }
        }
        hellos
    });
    Host { addr, stop, thread }
}

fn connect_to(client: &mut Client, host: SocketAddrV4) -> Result<HandshakeReply, NetError> {
    let cancel = AtomicBool::new(false);
    connect_with(
        &mut client.tx,
        &mut client.rx,
        host,
        &hello(),
        &cancel,
        INTERVAL,
        GIVE_UP,
    )
}

#[test]
fn welcome_after_two_lost_hellos() {
    let host = host(|n| if n < 2 { Vec::new() } else { vec![welcome()] });
    let mut client = client();
    let reply = connect_to(&mut client, host.addr).unwrap();
    assert_eq!(
        reply,
        HandshakeReply::Welcome {
            session: SessionId::new(5),
            mode: mode()
        }
    );
    let hellos = host.hellos();
    assert_eq!(hellos.len(), 3, "{hellos:?}");
    assert!(hellos[0] != hellos[1] && hellos[1] != hellos[2]);
}

#[test]
fn reject_is_returned() {
    let host = host(|_| {
        vec![encoded(&ControlMessage::Reject {
            reason: RejectReason::Busy,
        })]
    });
    let mut client = client();
    let reply = connect_to(&mut client, host.addr).unwrap();
    assert_eq!(reply, HandshakeReply::Reject(RejectReason::Busy));
}

#[test]
fn silent_host_times_out() {
    let host = host(|_| Vec::new());
    let mut client = client();
    let start = Instant::now();
    let result = connect_to(&mut client, host.addr);
    assert!(matches!(result, Err(NetError::Timeout)), "{result:?}");
    assert!(start.elapsed() >= GIVE_UP);
    assert!(start.elapsed() < Duration::from_millis(1500));
}

#[test]
fn cancel_stops_waiting() {
    let host = host(|_| Vec::new());
    let mut client = client();
    let cancel = Arc::new(AtomicBool::new(false));
    let setter = Arc::clone(&cancel);
    let canceller = thread::spawn(move || {
        thread::sleep(Duration::from_millis(100));
        setter.store(true, Ordering::Release);
    });
    let start = Instant::now();
    let result = connect_with(
        &mut client.tx,
        &mut client.rx,
        host.addr,
        &hello(),
        &cancel,
        INTERVAL,
        Duration::from_secs(10),
    );
    assert!(matches!(result, Err(NetError::Cancelled)), "{result:?}");
    assert!(start.elapsed() < Duration::from_millis(200));
    canceller.join().unwrap();
}

#[test]
fn reply_from_other_address_is_ignored() {
    let host = host(|_| {
        vec![encoded(&ControlMessage::Reject {
            reason: RejectReason::Version,
        })]
    });
    let mut client = client();
    let (_other, mut other_lanes) = Transport::start(TransportConfig::loopback()).unwrap();
    let mut other_tx = other_lanes.take_tx(Channel::Control).unwrap();
    send(
        &mut other_tx,
        &mut Packetizer::new(Channel::Control),
        1,
        &welcome(),
        client.addr,
    );
    thread::sleep(Duration::from_millis(20));
    let reply = connect_to(&mut client, host.addr).unwrap();
    assert_eq!(reply, HandshakeReply::Reject(RejectReason::Version));
}

#[test]
fn bye_is_unexpected() {
    let host = host(|_| {
        vec![encoded(&ControlMessage::Bye {
            reason: ByeReason::Shutdown,
        })]
    });
    let mut client = client();
    let result = connect_to(&mut client, host.addr);
    assert!(
        matches!(result, Err(NetError::UnexpectedReply)),
        "{result:?}"
    );
}

#[test]
fn garbage_is_ignored() {
    let host = host(|_| vec![b"{x".to_vec(), encoded(&hello()), welcome()]);
    let mut client = client();
    let reply = connect_to(&mut client, host.addr).unwrap();
    assert!(matches!(reply, HandshakeReply::Welcome { .. }), "{reply:?}");
}
