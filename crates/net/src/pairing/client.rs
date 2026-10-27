//! Client side of pairing (spec 0018): pure steps (`start`, `finish`) and blocking calls on a link.

use std::time::Duration;

use mixlink_core::ids::DeviceId;
use mixlink_proto::control::{Blob, ControlMessage, RejectReason};
use mixlink_proto::header::WIRE_VERSION;
use spake2::{Ed25519Group, Identity, Password, Spake2};

use super::transcript::{self, Parts};
use super::{KnownHost, PROMPT_TIME, PairError, pin_text};
use crate::control_link::{ControlLink, Incoming, Wire};
use crate::error::NetError;
use crate::keys::ClientIdentity;
use crate::resend::{HELLO_GIVE_UP, HELLO_RESEND};

/// Who pairs with whom. `host` and `host_name` come from discovery.
#[derive(Debug, Clone)]
pub struct PairContext {
    pub me: ClientIdentity,
    pub name: String,
    pub host: DeviceId,
    pub host_name: String,
}

impl PairContext {
    pub fn request(&self) -> ControlMessage {
        ControlMessage::PairRequest {
            proto: WIRE_VERSION,
            device: self.me.id,
            host: self.host,
            name: self.name.clone(),
            key: self.me.keys.public,
        }
    }
}

/// One PIN attempt in flight.
#[derive(Debug)]
pub struct PinAttempt {
    state: Spake2<Ed25519Group>,
    spake_a: Vec<u8>,
}

/// SPAKE2 message A for `pin`, as a `PairStart`.
pub fn start(ctx: &PairContext, pin: u32) -> (PinAttempt, ControlMessage) {
    let (state, spake_a) = Spake2::<Ed25519Group>::start_a(
        &Password::new(pin_text(pin).as_bytes()),
        &Identity::new(ctx.me.id.as_bytes()),
        &Identity::new(ctx.host.as_bytes()),
    );
    let msg = ControlMessage::PairStart {
        device: ctx.me.id,
        spake: Blob(spake_a.clone()),
    };
    (PinAttempt { state, spake_a }, msg)
}

/// Checks the host's `PairReply`; on success returns the `PairConfirm` to send and the host to store.
pub fn finish(
    ctx: &PairContext,
    attempt: PinAttempt,
    reply: &ControlMessage,
) -> Result<(ControlMessage, KnownHost), PairError> {
    let ControlMessage::PairReply { spake, key, mac } = reply else {
        return Err(reply_error(reply));
    };
    let k = attempt
        .state
        .finish(&spake.0)
        .map_err(|_| PairError::BadPin)?;
    let t = transcript::transcript(&Parts {
        device: &ctx.me.id,
        host: &ctx.host,
        client_key: &ctx.me.keys.public,
        host_key: key,
        spake_a: &attempt.spake_a,
        spake_b: &spake.0,
    });
    let (confirm_h, confirm_c) = transcript::confirm_keys(&k, &t);
    if !transcript::verify(&confirm_h, &t, &mac.0) {
        return Err(PairError::BadPin);
    }
    let confirm = ControlMessage::PairConfirm {
        device: ctx.me.id,
        mac: Blob(transcript::mac(&confirm_c, &t).to_vec()),
    };
    let host = KnownHost {
        host: ctx.host,
        name: ctx.host_name.clone(),
        key: *key,
    };
    Ok((confirm, host))
}

/// The error for a reply that is not the one expected.
pub fn reply_error(reply: &ControlMessage) -> PairError {
    match reply {
        ControlMessage::Reject { reason } => match reason {
            RejectReason::BadPin => PairError::BadPin,
            RejectReason::PairingClosed => PairError::Closed,
            RejectReason::Busy => PairError::Busy,
            RejectReason::Version => PairError::Version,
            RejectReason::NotPaired | RejectReason::BadDisplay => {
                PairError::Net(NetError::UnexpectedReply)
            }
        },
        _ => PairError::Net(NetError::UnexpectedReply),
    }
}

const TIMING: (Duration, Duration) = (HELLO_RESEND, HELLO_GIVE_UP);

/// Plaintext replies only: pairing happens before any session keys.
fn plain(incoming: Incoming) -> Option<ControlMessage> {
    (!incoming.sealed).then_some(incoming.msg)
}

/// Asks the host to show a PIN; returns how many seconds it stays valid.
pub fn request(
    link: &mut ControlLink,
    wire: &mut Wire<'_>,
    ctx: &PairContext,
) -> Result<u32, PairError> {
    let reply = link.exchange(wire, &ctx.request(), false, TIMING, |incoming| {
        plain(incoming).filter(|msg| {
            matches!(
                msg,
                ControlMessage::PairPrompt { .. } | ControlMessage::Reject { .. }
            )
        })
    })?;
    match reply {
        ControlMessage::PairPrompt { expires_s } => Ok(expires_s.min(PROMPT_TIME.as_secs() as u32)),
        other => Err(reply_error(&other)),
    }
}

/// Runs one PIN attempt to the end; on success the host is stored as paired on its side too.
pub fn submit(
    link: &mut ControlLink,
    wire: &mut Wire<'_>,
    ctx: &PairContext,
    pin: u32,
) -> Result<KnownHost, PairError> {
    let (attempt, start_msg) = start(ctx, pin);
    let reply = link.exchange(wire, &start_msg, false, TIMING, |incoming| {
        plain(incoming).filter(|msg| {
            matches!(
                msg,
                ControlMessage::PairReply { .. } | ControlMessage::Reject { .. }
            )
        })
    })?;
    let (confirm, host) = finish(ctx, attempt, &reply)?;
    let done = link.exchange(wire, &confirm, false, TIMING, |incoming| {
        plain(incoming).filter(|msg| {
            matches!(
                msg,
                ControlMessage::PairDone | ControlMessage::Reject { .. }
            )
        })
    })?;
    match done {
        ControlMessage::PairDone => Ok(host),
        other => Err(reply_error(&other)),
    }
}
