use std::time::{Duration, Instant};

use super::super::client::{self, PairContext};
use super::*;
use crate::keys::ClientIdentity;
use crate::pairing::PairError;

const PIN: u32 = 123_456;

struct Setup {
    host: HostPairing,
    ctx: PairContext,
    host_pub: PublicKey,
    t0: Instant,
}

fn setup() -> Setup {
    let host_keys = StaticKeyPair::generate().unwrap();
    let host_id = DeviceId::from_bytes([0xaa; 16]);
    let host_pub = host_keys.public;
    let ctx = PairContext {
        me: ClientIdentity {
            id: DeviceId::from_bytes([0x11; 16]),
            keys: StaticKeyPair::generate().unwrap(),
        },
        name: "Laptop".to_owned(),
        host: host_id,
        host_name: "Desk".to_owned(),
    };
    Setup {
        host: HostPairing::new(host_id, host_keys),
        ctx,
        host_pub,
        t0: Instant::now(),
    }
}

fn at(s: &Setup, secs: u64) -> Now {
    Now {
        mono: s.t0 + Duration::from_secs(secs),
        unix_s: 1_000 + secs,
    }
}

fn send(s: &mut Setup, msg: &ControlMessage, secs: u64) -> HostPairEvent {
    let now = at(s, secs);
    s.host.handle(msg, now, || Ok(PIN)).unwrap()
}

fn reply(s: &mut Setup, msg: &ControlMessage, secs: u64) -> ControlMessage {
    send(s, msg, secs).reply
}

fn rejected(reason: RejectReason) -> ControlMessage {
    ControlMessage::Reject { reason }
}

/// Request, then one PIN attempt; returns the host's answer to `PairStart` and the attempt.
fn attempt(s: &mut Setup, pin: u32) -> (client::PinAttempt, ControlMessage) {
    let (attempt, start) = client::start(&s.ctx, pin);
    let answer = reply(s, &start, 1);
    (attempt, answer)
}

#[test]
fn request_opens_prompt_with_pin() {
    let mut s = setup();
    let request = s.ctx.request();
    assert_eq!(
        reply(&mut s, &request, 0),
        ControlMessage::PairPrompt { expires_s: 120 }
    );
    let view = s.host.prompt(at(&s, 20)).unwrap();
    assert_eq!(view.pin, "123456");
    assert_eq!(view.device_name, "Laptop");
    assert_eq!(view.expires_s, 100);
}

#[test]
fn pin_is_zero_padded() {
    let mut s = setup();
    let request = s.ctx.request();
    let now = at(&s, 0);
    s.host.handle(&request, now, || Ok(42)).unwrap();
    assert_eq!(s.host.prompt(now).unwrap().pin, "000042");
}

#[test]
fn other_device_gets_busy() {
    let mut s = setup();
    let request = s.ctx.request();
    reply(&mut s, &request, 0);
    let mut other = s.ctx.clone();
    other.me.id = DeviceId::from_bytes([0x22; 16]);
    assert_eq!(
        reply(&mut s, &other.request(), 1),
        rejected(RejectReason::Busy)
    );
}

#[test]
fn wrong_host_id_closed() {
    let mut s = setup();
    let mut ctx = s.ctx.clone();
    ctx.host = DeviceId::from_bytes([0xbb; 16]);
    assert_eq!(
        reply(&mut s, &ctx.request(), 0),
        rejected(RejectReason::PairingClosed)
    );
    assert!(s.host.prompt(at(&s, 0)).is_none());
}

#[test]
fn right_pin_pairs() {
    let mut s = setup();
    let request = s.ctx.request();
    reply(&mut s, &request, 0);
    let (attempt, answer) = attempt(&mut s, PIN);
    let (confirm, known) = client::finish(&s.ctx, attempt, &answer).unwrap();
    assert_eq!(known.key, s.host_pub);
    assert_eq!(known.name, "Desk");
    let event = send(&mut s, &confirm, 2);
    assert_eq!(event.reply, ControlMessage::PairDone);
    let paired = event.paired.unwrap();
    assert_eq!(paired.device, s.ctx.me.id);
    assert_eq!(paired.key, s.ctx.me.keys.public);
    assert_eq!(paired.paired_at, 1_002);
    assert!(s.host.prompt(at(&s, 2)).is_none());
    // A resent confirm (lost `PairDone`) gets `PairDone` again, not a second record.
    let again = send(&mut s, &confirm, 3);
    assert_eq!(again.reply, ControlMessage::PairDone);
    assert!(again.paired.is_none());
}

#[test]
fn wrong_pin_bad_pin() {
    let mut s = setup();
    let request = s.ctx.request();
    reply(&mut s, &request, 0);
    let (attempt, answer) = attempt(&mut s, 654_321);
    assert!(matches!(
        client::finish(&s.ctx, attempt, &answer),
        Err(PairError::BadPin)
    ));
}

#[test]
fn forged_client_confirm_bad_pin() {
    let mut s = setup();
    let request = s.ctx.request();
    reply(&mut s, &request, 0);
    let (_, _) = attempt(&mut s, PIN);
    let forged = ControlMessage::PairConfirm {
        device: s.ctx.me.id,
        mac: Blob(vec![0; 32]),
    };
    let event = send(&mut s, &forged, 2);
    assert_eq!(event.reply, rejected(RejectReason::BadPin));
    assert!(event.paired.is_none());
}

#[test]
fn three_attempts_then_closed() {
    let mut s = setup();
    let request = s.ctx.request();
    reply(&mut s, &request, 0);
    for _ in 0..3 {
        let (_, answer) = attempt(&mut s, 1);
        assert!(matches!(answer, ControlMessage::PairReply { .. }));
    }
    let (_, answer) = attempt(&mut s, PIN);
    assert_eq!(answer, rejected(RejectReason::PairingClosed));
    assert!(s.host.prompt(at(&s, 1)).is_none());
}

#[test]
fn repeated_pair_start_is_not_an_attempt() {
    let mut s = setup();
    let request = s.ctx.request();
    reply(&mut s, &request, 0);
    let (attempt, start) = client::start(&s.ctx, PIN);
    let first = reply(&mut s, &start, 1);
    for _ in 0..5 {
        assert_eq!(reply(&mut s, &start, 1), first);
    }
    let (confirm, _) = client::finish(&s.ctx, attempt, &first).unwrap();
    assert_eq!(reply(&mut s, &confirm, 2), ControlMessage::PairDone);
}

#[test]
fn prompt_expires_after_120s() {
    let mut s = setup();
    let request = s.ctx.request();
    reply(&mut s, &request, 0);
    assert!(s.host.prompt(at(&s, 119)).is_some());
    assert!(s.host.prompt(at(&s, 120)).is_none());
    let (_, start) = client::start(&s.ctx, PIN);
    assert_eq!(
        reply(&mut s, &start, 121),
        rejected(RejectReason::PairingClosed)
    );
}

#[test]
fn cancel_closes_prompt() {
    let mut s = setup();
    let request = s.ctx.request();
    reply(&mut s, &request, 0);
    s.host.cancel();
    assert!(s.host.prompt(at(&s, 1)).is_none());
    let (_, start) = client::start(&s.ctx, PIN);
    assert_eq!(
        reply(&mut s, &start, 1),
        rejected(RejectReason::PairingClosed)
    );
}

#[test]
fn other_messages_are_not_pairing() {
    let mut s = setup();
    let now = at(&s, 0);
    assert!(
        s.host
            .handle(&ControlMessage::Ping { n: 1 }, now, || Ok(PIN))
            .is_none()
    );
}
