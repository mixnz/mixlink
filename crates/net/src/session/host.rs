//! Host side of the session handshake (spec 0018): pure, one Noise `KK` responder per `SessionInit`.

use std::net::SocketAddrV4;

use mixlink_core::ids::{DeviceId, SessionId};
use mixlink_proto::control::{self, Blob, CodecId, ControlMessage, DisplayInfo, RejectReason};
use mixlink_proto::crypto::SessionKeys;
use mixlink_proto::header::WIRE_VERSION;

use super::noise;
use crate::error::NetError;
use crate::keys::StaticKeyPair;
use crate::pairing::PairedDevice;

/// The fields of a `SessionInit`.
#[derive(Debug, Clone, Copy)]
pub struct Init<'a> {
    pub proto: u8,
    pub device: DeviceId,
    pub host: DeviceId,
    pub noise: &'a [u8],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InitOutcome {
    /// Not a valid handshake (bad key, bad payload): send nothing.
    Ignore,
    /// Send this and keep no session (reject, or a repeat of an answered init).
    Reply(ControlMessage),
    /// Send `reply`; the session exists once the client's first sealed datagram arrives.
    Accepted {
        reply: ControlMessage,
        session: SessionId,
        device: DeviceId,
        keys: SessionKeys,
        /// From the `Welcome` sent.
        mode: DisplayInfo,
        codec: CodecId,
    },
}

#[derive(Debug)]
struct Answered {
    peer: SocketAddrV4,
    init: Vec<u8>,
    reply: ControlMessage,
}

/// Last answered inits kept for resends.
const ANSWERED: usize = 8;

#[derive(Debug)]
pub struct HostSessions {
    host: DeviceId,
    keys: StaticKeyPair,
    paired: Vec<PairedDevice>,
    answered: Vec<Answered>,
}

impl HostSessions {
    pub fn new(host: DeviceId, keys: StaticKeyPair, paired: Vec<PairedDevice>) -> HostSessions {
        HostSessions {
            host,
            keys,
            paired,
            answered: Vec::new(),
        }
    }

    pub fn paired(&self) -> &[PairedDevice] {
        &self.paired
    }

    pub fn set_paired(&mut self, paired: Vec<PairedDevice>) {
        self.paired = paired;
    }

    /// Adds or replaces one paired device.
    pub fn add_paired(&mut self, device: PairedDevice) {
        self.paired.retain(|known| known.device != device.device);
        self.paired.push(device);
    }

    /// Answers a `SessionInit`. `choose` turns the client's `Hello` into `Welcome` (with `session`)
    /// or `Reject`.
    pub fn on_init(
        &mut self,
        peer: SocketAddrV4,
        init: &Init<'_>,
        session: SessionId,
        choose: impl FnOnce(&ControlMessage, SessionId) -> ControlMessage,
    ) -> InitOutcome {
        if init.proto != WIRE_VERSION {
            return InitOutcome::Reply(reject(RejectReason::Version));
        }
        let Some(client) = self
            .paired
            .iter()
            .find(|known| known.device == init.device && init.host == self.host)
        else {
            return InitOutcome::Reply(reject(RejectReason::NotPaired));
        };
        if let Some(done) = self
            .answered
            .iter()
            .find(|done| done.peer == peer && done.init == init.noise)
        {
            return InitOutcome::Reply(done.reply.clone());
        }
        let prologue = noise::prologue(init.device, self.host);
        let accepted =
            noise::responder(&self.keys, &client.key, &prologue).and_then(|mut state| {
                let payload = noise::read(&mut state, init.noise)?;
                let hello = control::decode(&payload)?;
                Ok((state, hello))
            });
        let (mut state, hello) = match accepted {
            Ok(found) => found,
            Err(error) => {
                log::debug!("session init from {peer} dropped: {error}");
                return InitOutcome::Ignore;
            }
        };
        if !matches!(&hello, ControlMessage::Hello { device, .. } if *device == init.device) {
            log::debug!("session init from {peer} carries no matching Hello");
            return InitOutcome::Ignore;
        }
        let inner = choose(&hello, session);
        let written = control::encode(&inner)
            .map_err(NetError::from)
            .and_then(|json| noise::write(&mut state, &json));
        let reply = match written {
            Ok(message) => ControlMessage::SessionAccept {
                noise: Blob(message),
            },
            Err(error) => {
                log::warn!("session accept for {peer} not written: {error}");
                return InitOutcome::Ignore;
            }
        };
        self.remember(peer, init.noise, &reply);
        match inner {
            ControlMessage::Welcome { mode, codec, .. } => InitOutcome::Accepted {
                reply,
                session,
                device: init.device,
                keys: noise::derive(state.dangerously_get_raw_split()),
                mode,
                codec,
            },
            _ => InitOutcome::Reply(reply),
        }
    }

    fn remember(&mut self, peer: SocketAddrV4, init: &[u8], reply: &ControlMessage) {
        if self.answered.len() >= ANSWERED {
            self.answered.remove(0);
        }
        self.answered.push(Answered {
            peer,
            init: init.to_vec(),
            reply: reply.clone(),
        });
    }
}

fn reject(reason: RejectReason) -> ControlMessage {
    ControlMessage::Reject { reason }
}

#[cfg(test)]
#[path = "host_tests.rs"]
mod tests;
