use std::net::Ipv4Addr;
use std::sync::atomic::AtomicBool;

use mixlink_net::control_link::{ControlLink, Wire};
use mixlink_net::error::NetError;
use mixlink_net::keys::ClientIdentity;
use mixlink_net::pairing::client::{self, PairContext};
use mixlink_net::session::client::connect_secure;
use mixlink_proto::control::{ControlMessage, DisplayInfo, RejectReason};
use mixlink_proto::header::WIRE_VERSION;

use super::*;

const HOST: DeviceId = DeviceId::from_bytes([0xaa; 16]);

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mixlink-session-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

struct Client {
    _transport: Transport,
    tx: mixlink_net::TxLane,
    rx: mixlink_net::RxLane,
    cancel: AtomicBool,
}

impl Client {
    fn new() -> Client {
        let (transport, mut lanes) = Transport::start(TransportConfig::loopback()).unwrap();
        Client {
            tx: lanes.take_tx(Channel::Control).unwrap(),
            rx: lanes.take_rx(Channel::Control).unwrap(),
            _transport: transport,
            cancel: AtomicBool::new(false),
        }
    }

    fn wire(&mut self) -> Wire<'_> {
        Wire {
            tx: &mut self.tx,
            rx: &mut self.rx,
            cancel: &self.cancel,
        }
    }
}

fn hello(me: &ClientIdentity) -> ControlMessage {
    ControlMessage::Hello {
        proto: WIRE_VERSION,
        device: me.id,
        name: "Laptop".to_owned(),
        displays: vec![DisplayInfo {
            width: 1920,
            height: 1080,
            refresh_mhz: 60_000,
            hdr: false,
            dpi: 96,
        }],
        stream_display: 0,
        codecs: vec![mixlink_proto::control::CodecId::H264],
    }
}

fn shown_pin(shared: &PairingShared) -> u32 {
    for _ in 0..200 {
        if let Some(prompt) = shared.prompt() {
            return prompt.pin.parse().unwrap();
        }
        thread::sleep(Duration::from_millis(10));
    }
    panic!("no prompt");
}

fn wait_for(check: impl Fn() -> bool) {
    for _ in 0..200 {
        if check() {
            return;
        }
        thread::sleep(Duration::from_millis(10));
    }
    panic!("condition not reached");
}

#[test]
fn pair_and_connect_over_loopback() {
    let secrets = temp_dir("loopback");
    std::fs::create_dir_all(&secrets).unwrap();
    let stop = Arc::new(StopSignal::new());
    let settings = SessionSettings {
        net: NetConfig {
            session_port: 0,
            ..NetConfig::default()
        },
        video: VideoConfig::default(),
        display: None,
        host: HOST,
        keys: StaticKeyPair::generate().unwrap(),
        secrets: secrets.clone(),
    };
    let listener = start(Arc::clone(&stop), settings).unwrap();
    let host_addr = SocketAddrV4::new(Ipv4Addr::LOCALHOST, listener.addr.port());

    let me = ClientIdentity {
        id: DeviceId::from_bytes([0x11; 16]),
        keys: StaticKeyPair::generate().unwrap(),
    };
    let ctx = PairContext {
        me: me.clone(),
        name: "Laptop".to_owned(),
        host: HOST,
        host_name: "Desk".to_owned(),
    };
    let mut c = Client::new();
    let mut link = ControlLink::new(host_addr);
    client::request(&mut link, &mut c.wire(), &ctx).unwrap();
    let pin = shown_pin(&listener.shared);
    let known = client::submit(&mut link, &mut c.wire(), &ctx, pin).unwrap();

    wait_for(|| listener.shared.paired().len() == 1);
    assert_eq!(secrets::load_paired(&secrets).len(), 1);
    assert!(listener.shared.prompt().is_none());

    let mut session = connect_secure(link, &mut c.wire(), &me, &known, &hello(&me)).unwrap();
    session.ping(&mut c.wire()).unwrap();
    session.bye(&mut c.wire()).unwrap();

    listener.shared.remove(me.id);
    wait_for(|| listener.shared.paired().is_empty());
    assert!(secrets::load_paired(&secrets).is_empty());
    let mut again = Client::new();
    let result = connect_secure(
        ControlLink::new(host_addr),
        &mut again.wire(),
        &me,
        &known,
        &hello(&me),
    );
    assert!(matches!(
        result,
        Err(NetError::Rejected(RejectReason::NotPaired))
    ));

    stop.stop();
    listener.thread.join().unwrap();
}

/// Answers display requests like `svc-display`: the monitor is "up" on `connector`.
fn fake_display(connector: u32) -> DisplayHandle {
    use crate::display::requests::{DisplayRequest, StreamTarget};
    use crate::display::session::Active;
    let (handle, inbox) = DisplayHandle::channel();
    thread::spawn(move || {
        for request in inbox {
            match request {
                DisplayRequest::Begin { client, reply, .. } => {
                    let _ = reply.send(Ok(StreamTarget {
                        active: Active { monitor: 9, client },
                        connector,
                    }));
                }
                DisplayRequest::End { reply, .. } => {
                    let _ = reply.send(Ok(()));
                }
            }
        }
    });
    handle
}

#[test]
fn stream_failure_sends_bye_host_error() {
    let secrets = temp_dir("stream-failure");
    std::fs::create_dir_all(&secrets).unwrap();
    let me = ClientIdentity {
        id: DeviceId::from_bytes([0x22; 16]),
        keys: StaticKeyPair::generate().unwrap(),
    };
    let host_keys = StaticKeyPair::generate().unwrap();
    let host_pub = host_keys.public;
    secrets::save_paired(
        &secrets,
        &[PairedDevice {
            device: me.id,
            name: "Laptop".to_owned(),
            key: me.keys.public,
            paired_at: 0,
        }],
    )
    .unwrap();
    let stop = Arc::new(StopSignal::new());
    let settings = SessionSettings {
        net: NetConfig {
            session_port: 0,
            ..NetConfig::default()
        },
        video: VideoConfig::default(),
        // Connector 3 has no frame share here, so capture fails right after the monitor is "up".
        display: Some(fake_display(3)),
        host: HOST,
        keys: host_keys,
        secrets,
    };
    let listener = start(Arc::clone(&stop), settings).unwrap();
    let host_addr = SocketAddrV4::new(Ipv4Addr::LOCALHOST, listener.addr.port());
    let known = mixlink_net::pairing::KnownHost {
        host: HOST,
        name: "Desk".to_owned(),
        key: host_pub,
    };
    let mut c = Client::new();
    let mut session = connect_secure(
        ControlLink::new(host_addr),
        &mut c.wire(),
        &me,
        &known,
        &hello(&me),
    )
    .unwrap();
    let mut bye = None;
    for _ in 0..40 {
        if let Some(mut dg) = c.rx.recv_timeout(Duration::from_millis(100))
            && let Some(incoming) = session.link.accept(&mut dg)
            && let ControlMessage::Bye { reason } = incoming.msg
        {
            bye = Some((reason, incoming.sealed));
            break;
        }
    }
    assert_eq!(
        bye,
        Some((mixlink_proto::control::ByeReason::HostError, true))
    );
    stop.stop();
    listener.thread.join().unwrap();
}
