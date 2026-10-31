use std::sync::mpsc;
use std::time::SystemTime;

use mixlink_core::config::CodecChoice;
use mixlink_core::ids::DeviceId;
use mixlink_net::endpoint::{EndpointEvent, HostControl, HostControlConfig};
use mixlink_net::keys::StaticKeyPair;
use mixlink_net::mode_policy::ModePolicy;
use mixlink_net::pairing::{Now, PairedDevice};
use mixlink_proto::control::ByeReason;
use mixlink_proto::header::WIRE_VERSION;

use super::*;
use crate::au_queue::channel;
use crate::net::cursor_state::cursor_channel;

const HOST: DeviceId = DeviceId::from_bytes([0xaa; 16]);

/// What the test host does once the session started.
#[derive(Clone, Copy)]
enum After {
    GoQuiet,
    SayBye,
    Serve,
}

fn now() -> Now {
    Now {
        mono: Instant::now(),
        unix_s: SystemTime::UNIX_EPOCH.elapsed().map_or(0, |d| d.as_secs()),
    }
}

/// A host on loopback that pairs `me` and answers handshakes; reports its events.
fn host(
    me: &ClientIdentity,
    after: After,
) -> (KnownHost, SocketAddrV4, mpsc::Receiver<EndpointEvent>) {
    let (transport, mut lanes) = Transport::start(TransportConfig::loopback()).unwrap();
    let addr = transport.local_addr();
    let keys = StaticKeyPair::generate().unwrap();
    let known = KnownHost {
        host: HOST,
        name: "Desk".to_owned(),
        key: keys.public,
    };
    let mut control = HostControl::new(HostControlConfig {
        host: HOST,
        keys,
        paired: vec![PairedDevice {
            device: me.id,
            name: "Laptop".to_owned(),
            key: me.keys.public,
            paired_at: 0,
        }],
        policy: ModePolicy {
            max_fps: 120,
            codec: CodecChoice::Auto,
        },
    });
    let (events_tx, events) = mpsc::channel();
    let device = me.id;
    thread::spawn(move || {
        let _transport = transport;
        let mut tx = lanes.take_tx(Channel::Control).unwrap();
        let mut rx = lanes.take_rx(Channel::Control).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut quiet = false;
        while Instant::now() < deadline {
            let Some(mut dg) = rx.recv_timeout(Duration::from_millis(20)) else {
                continue;
            };
            if quiet {
                continue;
            }
            for event in control.on_datagram(&mut tx, &mut dg, now()) {
                let started = matches!(event, EndpointEvent::SessionStarted { .. });
                let _ = events_tx.send(event);
                if started {
                    match after {
                        After::GoQuiet => quiet = true,
                        After::SayBye => {
                            control.end_session(&mut tx, device, ByeReason::HostError);
                        }
                        After::Serve => {}
                    }
                }
            }
        }
    });
    (known, addr, events)
}

fn identity() -> ClientIdentity {
    ClientIdentity {
        id: DeviceId::from_bytes([0x11; 16]),
        keys: StaticKeyPair::generate().unwrap(),
    }
}

fn plan(me: ClientIdentity, host: KnownHost, addr: SocketAddrV4) -> SessionPlan {
    let hello = ControlMessage::Hello {
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
        codecs: vec![CodecId::H264, CodecId::Hevc],
    };
    SessionPlan {
        me,
        host,
        addr,
        hello,
        drop: DropKnob::default(),
        silence: Duration::from_millis(300),
    }
}

fn io() -> (SessionIo, Arc<OnceLock<VideoCodec>>, Arc<AtomicBool>) {
    let (au, _rx) = channel(2, 1024);
    let (cursor, _cursor_rx) = cursor_channel();
    let codec = Arc::new(OnceLock::new());
    let stop = Arc::new(AtomicBool::new(false));
    let io = SessionIo {
        au,
        cursor,
        codec: Arc::clone(&codec),
        want_keyframe: Arc::default(),
        stop: Arc::clone(&stop),
        input: std::sync::mpsc::sync_channel(1).0,
    };
    (io, codec, stop)
}

#[test]
fn silence_is_connection_lost() {
    let me = identity();
    let (known, addr, _events) = host(&me, After::GoQuiet);
    let (io, codec, _stop) = io();
    let mut connected = None;
    let end = run(&plan(me, known, addr), io, |mode, codec| {
        connected = Some((mode.width, codec));
    });
    assert_eq!(end.reason, Some(CloseReason::ConnectionLost));
    assert_eq!(connected, Some((1920, CodecId::Hevc)));
    assert_eq!(codec.get(), Some(&VideoCodec::Hevc));
}

#[test]
fn host_bye_is_host_ended() {
    let me = identity();
    let (known, addr, _events) = host(&me, After::SayBye);
    let (io, _, _) = io();
    let end = run(&plan(me, known, addr), io, |_, _| {});
    assert_eq!(
        end.reason,
        Some(CloseReason::HostEnded {
            reason: "hostError".to_owned()
        })
    );
}

#[test]
fn stop_sends_bye() {
    let me = identity();
    let (known, addr, events) = host(&me, After::Serve);
    let (io, _, stop) = io();
    let plan = plan(me, known, addr);
    let client = thread::spawn(move || run(&plan, io, |_, _| {}));
    assert!(matches!(
        events.recv_timeout(Duration::from_secs(3)),
        Ok(EndpointEvent::SessionStarted { .. })
    ));
    stop.store(true, Ordering::Release);
    assert_eq!(client.join().unwrap().reason, None);
    assert!(matches!(
        events.recv_timeout(Duration::from_secs(3)),
        Ok(EndpointEvent::SessionEnded { .. })
    ));
}

#[test]
fn unknown_client_is_not_paired() {
    let me = identity();
    let (known, addr, _events) = host(&me, After::Serve);
    let (io, _, _) = io();
    let stranger = ClientIdentity {
        id: DeviceId::from_bytes([0x22; 16]),
        keys: StaticKeyPair::generate().unwrap(),
    };
    let end = run(&plan(stranger, known, addr), io, |_, _| {});
    assert_eq!(end.reason, Some(CloseReason::NotPaired));
}
