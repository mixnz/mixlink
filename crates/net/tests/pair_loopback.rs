//! Spec 0018 end to end on loopback: pair with the PIN, connect a secure session, ping, bye.
//! The host runs `HostControl` on its own thread, as the service does.

// Helpers outside `#[test]` fns are not covered by `allow-unwrap-in-tests`.
#![allow(clippy::unwrap_used)]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use mixlink_core::ids::DeviceId;
use mixlink_net::control_link::{ControlLink, Wire};
use mixlink_net::endpoint::{EndpointEvent, HostControl, HostControlConfig};
use mixlink_net::error::NetError;
use mixlink_net::keys::{ClientIdentity, StaticKeyPair};
use mixlink_net::pairing::client::{self, PairContext};
use mixlink_net::pairing::{KnownHost, Now, PairError, PromptView};
use mixlink_net::session::client::connect_secure;
use mixlink_net::{Transport, TransportConfig};
use mixlink_proto::channel::Channel;
use mixlink_proto::control::{ControlMessage, DisplayInfo, RejectReason};
use mixlink_proto::header::WIRE_VERSION;

const HOST: DeviceId = DeviceId::from_bytes([0xaa; 16]);

struct Host {
    addr: std::net::SocketAddrV4,
    prompt: Arc<Mutex<Option<PromptView>>>,
    events: Receiver<EndpointEvent>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for Host {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn now() -> Now {
    let unix_s = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    Now {
        mono: Instant::now(),
        unix_s,
    }
}

fn start_host() -> Host {
    let (transport, mut lanes) = Transport::start(TransportConfig::loopback()).unwrap();
    let addr = transport.local_addr();
    let mut tx = lanes.take_tx(Channel::Control).unwrap();
    let mut rx = lanes.take_rx(Channel::Control).unwrap();
    let prompt = Arc::new(Mutex::new(None));
    let stop = Arc::new(AtomicBool::new(false));
    let (events_tx, events) = mpsc::channel();
    let thread = {
        let prompt = Arc::clone(&prompt);
        let stop = Arc::clone(&stop);
        thread::spawn(move || {
            let _transport = transport;
            let mut host = HostControl::new(HostControlConfig {
                host: HOST,
                keys: StaticKeyPair::generate().unwrap(),
                paired: Vec::new(),
            });
            while !stop.load(Ordering::Acquire) {
                let mut found = Vec::new();
                if let Some(mut dg) = rx.recv_timeout(Duration::from_millis(20)) {
                    found = host.on_datagram(&mut tx, &mut dg, now());
                }
                found.extend(host.tick(now()));
                *prompt.lock().unwrap() = host.prompt(now());
                for event in found {
                    let _ = events_tx.send(event);
                }
            }
        })
    };
    Host {
        addr,
        prompt,
        events,
        stop,
        thread: Some(thread),
    }
}

struct Client {
    _transport: Transport,
    tx: mixlink_net::TxLane,
    rx: mixlink_net::RxLane,
    cancel: AtomicBool,
}

fn client() -> Client {
    let (transport, mut lanes) = Transport::start(TransportConfig::loopback()).unwrap();
    Client {
        tx: lanes.take_tx(Channel::Control).unwrap(),
        rx: lanes.take_rx(Channel::Control).unwrap(),
        _transport: transport,
        cancel: AtomicBool::new(false),
    }
}

impl Client {
    fn wire(&mut self) -> Wire<'_> {
        Wire {
            tx: &mut self.tx,
            rx: &mut self.rx,
            cancel: &self.cancel,
        }
    }
}

fn context() -> PairContext {
    PairContext {
        me: ClientIdentity {
            id: DeviceId::from_bytes([0x11; 16]),
            keys: StaticKeyPair::generate().unwrap(),
        },
        name: "Laptop".to_owned(),
        host: HOST,
        host_name: "Desk".to_owned(),
    }
}

fn hello(ctx: &PairContext) -> ControlMessage {
    ControlMessage::Hello {
        proto: WIRE_VERSION,
        device: ctx.me.id,
        name: ctx.name.clone(),
        displays: vec![DisplayInfo {
            width: 1920,
            height: 1080,
            refresh_mhz: 60_000,
            hdr: false,
            dpi: 96,
        }],
        stream_display: 0,
    }
}

fn shown_pin(host: &Host) -> u32 {
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        if let Some(view) = host.prompt.lock().unwrap().clone() {
            return view.pin.parse().unwrap();
        }
        thread::sleep(Duration::from_millis(10));
    }
    panic!("no prompt shown");
}

fn pair(host: &Host, c: &mut Client, ctx: &PairContext, link: &mut ControlLink) -> KnownHost {
    let expires = client::request(link, &mut c.wire(), ctx).unwrap();
    assert!(expires > 100 && expires <= 120);
    let pin = shown_pin(host);
    client::submit(link, &mut c.wire(), ctx, pin).unwrap()
}

#[test]
fn pair_then_connect_then_ping() {
    let host = start_host();
    let ctx = context();
    let mut c = client();
    let mut link = ControlLink::new(host.addr);
    let known = pair(&host, &mut c, &ctx, &mut link);
    assert!(matches!(
        host.events.recv_timeout(Duration::from_secs(2)).unwrap(),
        EndpointEvent::Paired(device) if device.device == ctx.me.id
    ));
    let mut session = connect_secure(link, &mut c.wire(), &ctx.me, &known, &hello(&ctx)).unwrap();
    assert_eq!(session.mode.width, 1920);
    assert!(matches!(
        host.events.recv_timeout(Duration::from_secs(2)).unwrap(),
        EndpointEvent::SessionStarted { session: id, .. } if id == session.id
    ));
    for _ in 0..5 {
        let rtt = session.ping(&mut c.wire()).unwrap();
        assert!(rtt < Duration::from_millis(200), "rtt {rtt:?}");
    }
    session.bye(&mut c.wire()).unwrap();
    assert!(matches!(
        host.events.recv_timeout(Duration::from_secs(2)).unwrap(),
        EndpointEvent::SessionEnded { .. }
    ));
}

#[test]
fn wrong_pin_then_right_pin() {
    let host = start_host();
    let ctx = context();
    let mut c = client();
    let mut link = ControlLink::new(host.addr);
    client::request(&mut link, &mut c.wire(), &ctx).unwrap();
    let pin = shown_pin(&host);
    let wrong = (pin + 1) % 1_000_000;
    assert!(matches!(
        client::submit(&mut link, &mut c.wire(), &ctx, wrong),
        Err(PairError::BadPin)
    ));
    assert!(client::submit(&mut link, &mut c.wire(), &ctx, pin).is_ok());
}

#[test]
fn unpaired_connect_gets_not_paired() {
    let host = start_host();
    let ctx = context();
    let mut c = client();
    let known = KnownHost {
        host: HOST,
        name: "Desk".to_owned(),
        key: StaticKeyPair::generate().unwrap().public,
    };
    let result = connect_secure(
        ControlLink::new(host.addr),
        &mut c.wire(),
        &ctx.me,
        &known,
        &hello(&ctx),
    );
    assert!(matches!(
        result,
        Err(NetError::Rejected(RejectReason::NotPaired))
    ));
}
