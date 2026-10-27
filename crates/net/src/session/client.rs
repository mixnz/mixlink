//! Client side of the session handshake (spec 0018): pure steps and the blocking `connect_secure`.

use std::time::{Duration, Instant};

use mixlink_core::ids::SessionId;
use mixlink_proto::control::{self, Blob, ByeReason, ControlMessage, DisplayInfo};
use mixlink_proto::crypto::{Role, SessionKeys};
use mixlink_proto::header::WIRE_VERSION;
use snow::HandshakeState;

use super::noise;
use crate::control_link::{ControlLink, Wire};
use crate::error::NetError;
use crate::keys::ClientIdentity;
use crate::pairing::KnownHost;
use crate::resend::{HELLO_GIVE_UP, HELLO_RESEND};

const TIMING: (Duration, Duration) = (HELLO_RESEND, HELLO_GIVE_UP);

/// A handshake waiting for the host's `SessionAccept`.
#[derive(Debug)]
pub struct SessionStart {
    state: HandshakeState,
}

/// Noise message 1 carrying `hello`, as a `SessionInit`.
pub fn init(
    me: &ClientIdentity,
    host: &KnownHost,
    hello: &ControlMessage,
) -> Result<(SessionStart, ControlMessage), NetError> {
    let prologue = noise::prologue(me.id, host.host);
    let mut state = noise::initiator(&me.keys, &host.key, &prologue)?;
    let message = noise::write(&mut state, &control::encode(hello)?)?;
    let msg = ControlMessage::SessionInit {
        proto: WIRE_VERSION,
        device: me.id,
        host: host.host,
        noise: Blob(message),
    };
    Ok((SessionStart { state }, msg))
}

/// Reads the host's answer: session keys and its `Welcome`, or the reason it refused.
pub fn finish(
    start: SessionStart,
    reply: &ControlMessage,
) -> Result<(SessionKeys, SessionId, DisplayInfo), NetError> {
    let noise_msg = match reply {
        ControlMessage::SessionAccept { noise } => noise,
        ControlMessage::Reject { reason } => return Err(NetError::Rejected(*reason)),
        _ => return Err(NetError::UnexpectedReply),
    };
    let mut state = start.state;
    let payload = noise::read(&mut state, &noise_msg.0)?;
    match control::decode(&payload)? {
        ControlMessage::Welcome { session, mode } => Ok((
            noise::derive(state.dangerously_get_raw_split()),
            session,
            mode,
        )),
        ControlMessage::Reject { reason } => Err(NetError::Rejected(reason)),
        _ => Err(NetError::UnexpectedReply),
    }
}

/// An authenticated session; its control link seals every message.
#[derive(Debug)]
pub struct SecureSession {
    pub id: SessionId,
    pub mode: DisplayInfo,
    pub keys: SessionKeys,
    pub link: ControlLink,
    next_ping: u32,
}

/// Runs the handshake on `link`, then one sealed `Ping` so the host commits the session.
pub fn connect_secure(
    mut link: ControlLink,
    wire: &mut Wire<'_>,
    me: &ClientIdentity,
    host: &KnownHost,
    hello: &ControlMessage,
) -> Result<SecureSession, NetError> {
    let (start, init_msg) = init(me, host, hello)?;
    let reply = link.exchange(wire, &init_msg, false, TIMING, |incoming| {
        let accept = matches!(
            incoming.msg,
            ControlMessage::SessionAccept { .. } | ControlMessage::Reject { .. }
        );
        (!incoming.sealed && accept).then_some(incoming.msg)
    })?;
    let (keys, id, mode) = finish(start, &reply)?;
    link.set_keys(&keys, Role::Client);
    let mut session = SecureSession {
        id,
        mode,
        keys,
        link,
        next_ping: 0,
    };
    session.ping(wire)?;
    Ok(session)
}

impl SecureSession {
    /// Sends a sealed `Ping` until its `Pong` arrives; returns the time from the first send (a lost
    /// ping adds the 250 ms resend interval).
    pub fn ping(&mut self, wire: &mut Wire<'_>) -> Result<Duration, NetError> {
        let n = self.next_ping;
        self.next_ping = self.next_ping.wrapping_add(1);
        let sent = Instant::now();
        self.link.exchange(
            wire,
            &ControlMessage::Ping { n },
            true,
            TIMING,
            |incoming| {
                (incoming.sealed && incoming.msg == ControlMessage::Pong { n }).then_some(())
            },
        )?;
        Ok(sent.elapsed())
    }

    /// Tells the host the session ends; sent once (the host also times out after 10 s).
    pub fn bye(&mut self, wire: &mut Wire<'_>) -> Result<(), NetError> {
        self.link.send_sealed(
            wire.tx,
            &ControlMessage::Bye {
                reason: ByeReason::UserClosed,
            },
        )
    }
}
