use std::net::{Ipv4Addr, SocketAddrV4};

use mixlink_proto::control::DisplayInfo;

use super::super::client;
use super::*;
use crate::error::NetError;
use crate::keys::ClientIdentity;
use crate::pairing::KnownHost;

const HOST: DeviceId = DeviceId::from_bytes([0xaa; 16]);

struct Setup {
    sessions: HostSessions,
    me: ClientIdentity,
    known: KnownHost,
}

fn setup() -> Setup {
    let host_keys = StaticKeyPair::generate().unwrap();
    let me = ClientIdentity {
        id: DeviceId::from_bytes([0x11; 16]),
        keys: StaticKeyPair::generate().unwrap(),
    };
    let known = KnownHost {
        host: HOST,
        name: "Desk".to_owned(),
        key: host_keys.public,
    };
    let paired = vec![PairedDevice {
        device: me.id,
        name: "Laptop".to_owned(),
        key: me.keys.public,
        paired_at: 0,
    }];
    Setup {
        sessions: HostSessions::new(HOST, host_keys, paired),
        me,
        known,
    }
}

fn mode() -> DisplayInfo {
    DisplayInfo {
        width: 1920,
        height: 1080,
        refresh_mhz: 60_000,
        hdr: false,
        dpi: 96,
    }
}

fn hello(device: DeviceId) -> ControlMessage {
    ControlMessage::Hello {
        proto: WIRE_VERSION,
        device,
        name: "Laptop".to_owned(),
        displays: vec![mode()],
        stream_display: 0,
    }
}

fn peer() -> SocketAddrV4 {
    SocketAddrV4::new(Ipv4Addr::LOCALHOST, 5000)
}

fn welcome(_: &ControlMessage, session: SessionId) -> ControlMessage {
    ControlMessage::Welcome {
        session,
        mode: mode(),
    }
}

fn noise_of(msg: &ControlMessage) -> Vec<u8> {
    match msg {
        ControlMessage::SessionInit { noise, .. } => noise.0.clone(),
        other => panic!("not an init: {other:?}"),
    }
}

fn init_for(s: &Setup) -> (client::SessionStart, Vec<u8>) {
    let (start, msg) = client::init(&s.me, &s.known, &hello(s.me.id)).unwrap();
    (start, noise_of(&msg))
}

fn run(s: &mut Setup, device: DeviceId, proto: u8, noise: &[u8]) -> InitOutcome {
    let init = Init {
        proto,
        device,
        host: HOST,
        noise,
    };
    s.sessions
        .on_init(peer(), &init, SessionId::new(77), welcome)
}

#[test]
fn welcome_keys_match() {
    let mut s = setup();
    let (start, noise) = init_for(&s);
    let device = s.me.id;
    let InitOutcome::Accepted {
        reply,
        session,
        device: got,
        keys,
    } = run(&mut s, device, WIRE_VERSION, &noise)
    else {
        panic!("not accepted");
    };
    assert_eq!(session, SessionId::new(77));
    assert_eq!(got, device);
    let (client_keys, id, got_mode) = client::finish(start, &reply).unwrap();
    assert_eq!(client_keys, keys);
    assert_ne!(keys.c2h, keys.h2c);
    assert_eq!(id, SessionId::new(77));
    assert_eq!(got_mode, mode());
}

#[test]
fn unpaired_gets_not_paired() {
    let mut s = setup();
    let (_, noise) = init_for(&s);
    let stranger = DeviceId::from_bytes([0x33; 16]);
    assert_eq!(
        run(&mut s, stranger, WIRE_VERSION, &noise),
        InitOutcome::Reply(reject(RejectReason::NotPaired))
    );
}

#[test]
fn version_mismatch_rejected() {
    let mut s = setup();
    let (_, noise) = init_for(&s);
    let device = s.me.id;
    assert_eq!(
        run(&mut s, device, WIRE_VERSION + 1, &noise),
        InitOutcome::Reply(reject(RejectReason::Version))
    );
}

#[test]
fn repeated_init_gets_cached_accept() {
    let mut s = setup();
    let (_, noise) = init_for(&s);
    let device = s.me.id;
    let InitOutcome::Accepted { reply, .. } = run(&mut s, device, WIRE_VERSION, &noise) else {
        panic!("not accepted");
    };
    assert_eq!(
        run(&mut s, device, WIRE_VERSION, &noise),
        InitOutcome::Reply(reply)
    );
}

#[test]
fn forged_key_ignored() {
    let mut s = setup();
    // Same device id, another static key: the host's stored key does not match.
    s.me.keys = StaticKeyPair::generate().unwrap();
    let (_, noise) = init_for(&s);
    let device = s.me.id;
    assert_eq!(
        run(&mut s, device, WIRE_VERSION, &noise),
        InitOutcome::Ignore
    );
}

#[test]
fn client_with_wrong_host_key_fails() {
    let mut s = setup();
    s.known.key = StaticKeyPair::generate().unwrap().public;
    let (_, noise) = init_for(&s);
    let device = s.me.id;
    assert_eq!(
        run(&mut s, device, WIRE_VERSION, &noise),
        InitOutcome::Ignore
    );
}

#[test]
fn reject_inside_message_two() {
    let mut s = setup();
    let (start, noise) = init_for(&s);
    let init = Init {
        proto: WIRE_VERSION,
        device: s.me.id,
        host: HOST,
        noise: &noise,
    };
    let outcome = s
        .sessions
        .on_init(peer(), &init, SessionId::new(1), |_, _| {
            reject(RejectReason::BadDisplay)
        });
    let InitOutcome::Reply(reply) = outcome else {
        panic!("expected a reply without a session");
    };
    assert!(matches!(
        client::finish(start, &reply),
        Err(NetError::Rejected(RejectReason::BadDisplay))
    ));
}
