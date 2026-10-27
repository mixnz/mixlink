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
