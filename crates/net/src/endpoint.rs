//! Host control endpoint (spec 0018): pairing and session handshakes for every peer on the
//! session port's control lane. The service's `svc-session` thread drives it. Control plane.

use std::net::SocketAddrV4;
use std::time::Duration;

use mixlink_core::ids::{DeviceId, SessionId};
use mixlink_proto::control::{ByeReason, CodecId, ControlMessage, DisplayInfo, RejectReason};
use mixlink_proto::crypto::{Role, SessionKeys};

use crate::control_link::ControlLink;
use crate::endpoint_peer::{CLOSE_NOTICES, Live, Peer};
use crate::keys::{StaticKeyPair, random_pin, random_session};
use crate::lane::{RxDatagram, TxLane};
use crate::mode_policy::ModePolicy;
use crate::pairing::host::HostPairing;
use crate::pairing::{Now, PairedDevice, PromptView};
use crate::session::SESSION_TIMEOUT;
use crate::session::host::{HostSessions, Init, InitOutcome};

/// Peers tracked at once; the least recently seen one is dropped for a new one.
const MAX_PEERS: usize = 8;
/// A peer without a session is forgotten after this long.
const IDLE_PEER: Duration = Duration::from_secs(60);
#[derive(Debug, Clone)]
pub struct HostControlConfig {
    pub host: DeviceId,
    pub keys: StaticKeyPair,
    pub paired: Vec<PairedDevice>,
    pub policy: ModePolicy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EndpointEvent {
    Paired(PairedDevice),
    /// The client proved its keys with a sealed datagram. `keys` seal the media lanes; `mode` and
    /// `codec` are what `Welcome` promised (spec 0020).
    SessionStarted {
        session: SessionId,
        device: DeviceId,
        peer: SocketAddrV4,
        keys: SessionKeys,
        mode: DisplayInfo,
        codec: CodecId,
    },
    SessionEnded {
        session: SessionId,
        device: DeviceId,
    },
}

#[derive(Debug)]
pub struct HostControl {
    pairing: HostPairing,
    sessions: HostSessions,
    peers: Vec<Peer>,
    /// Address of the client waiting on the open prompt; that peer is kept past `IDLE_PEER`.
    prompt_peer: Option<SocketAddrV4>,
    policy: ModePolicy,
}

impl HostControl {
    pub fn new(config: HostControlConfig) -> HostControl {
        HostControl {
            pairing: HostPairing::new(config.host, config.keys.clone()),
            sessions: HostSessions::new(config.host, config.keys, config.paired),
            peers: Vec::new(),
            prompt_peer: None,
            policy: config.policy,
        }
    }

    pub fn paired(&self) -> &[PairedDevice] {
        self.sessions.paired()
    }

    /// Replaces the paired list (a device removed in the tray); its sessions end.
    pub fn set_paired(&mut self, paired: Vec<PairedDevice>) -> Vec<EndpointEvent> {
        let mut events = Vec::new();
        for peer in &mut self.peers {
            let gone = peer
                .live
                .as_ref()
                .is_some_and(|live| !paired.iter().any(|known| known.device == live.device));
            if gone {
                events.extend(peer.end());
            }
        }
        self.sessions.set_paired(paired);
        events
    }

    pub fn prompt(&self, now: Now) -> Option<PromptView> {
        self.pairing.prompt(now)
    }

    /// Closes the prompt and tells the waiting client with `Reject(PairingClosed)`.
    pub fn cancel_prompt(&mut self, tx: &mut TxLane) {
        let addr = self.prompt_peer.take();
        if !self.pairing.cancel() {
            return;
        }
        let Some(peer) = self
            .peers
            .iter_mut()
            .find(|peer| Some(peer.link.peer()) == addr)
        else {
            return;
        };
        let notice = ControlMessage::Reject {
            reason: RejectReason::PairingClosed,
        };
        for _ in 0..CLOSE_NOTICES {
            if let Err(error) = peer.link.send(tx, &notice) {
                log::debug!("close notice not sent: {error}");
            }
        }
    }

    /// Ends `device`'s session from the host side (spec 0020): sealed `Bye { reason }`, then the
    /// keys go. `None` when it has no session.
    pub fn end_session(
        &mut self,
        tx: &mut TxLane,
        device: DeviceId,
        reason: ByeReason,
    ) -> Option<EndpointEvent> {
        let peer = self
            .peers
            .iter_mut()
            .find(|peer| peer.live.as_ref().is_some_and(|live| live.device == device))?;
        peer.say_bye(tx, reason);
        peer.end()
    }

    /// Ends every session with `Bye { reason }` (service stop).
    pub fn end_all(&mut self, tx: &mut TxLane, reason: ByeReason) -> Vec<EndpointEvent> {
        let mut events = Vec::new();
        for peer in self.peers.iter_mut().filter(|peer| peer.live.is_some()) {
            peer.say_bye(tx, reason);
            events.extend(peer.end());
        }
        events
    }

    /// Handles one datagram from the control lane.
    pub fn on_datagram(
        &mut self,
        tx: &mut TxLane,
        dg: &mut RxDatagram<'_>,
        now: Now,
    ) -> Vec<EndpointEvent> {
        let index = self.peer_index(dg.peer(), now);
        let Some(incoming) = self.peers[index].link.accept(dg) else {
            return Vec::new();
        };
        self.peers[index].last_seen = now;
        if incoming.sealed {
            return self.on_sealed(tx, index, incoming.msg, now);
        }
        self.on_plain(tx, index, &incoming.msg, now)
    }

    /// Ends sessions with no valid datagram for 10 s, closes an expired prompt, forgets idle peers.
    pub fn tick(&mut self, now: Now) -> Vec<EndpointEvent> {
        self.pairing.expire(now);
        if !self.pairing.is_open() {
            self.prompt_peer = None;
        }
        let mut events = Vec::new();
        for peer in &mut self.peers {
            let stale = peer.live.as_ref().is_some_and(|live| {
                now.mono.saturating_duration_since(live.last_valid.mono) >= SESSION_TIMEOUT
            });
            if stale {
                events.extend(peer.end());
            }
        }
        self.peers.retain(|peer| {
            peer.live.is_some()
                || Some(peer.link.peer()) == self.prompt_peer
                || now.mono.saturating_duration_since(peer.last_seen.mono) < IDLE_PEER
        });
        events
    }

    fn peer_index(&mut self, addr: SocketAddrV4, now: Now) -> usize {
        if let Some(index) = self.peers.iter().position(|peer| peer.link.peer() == addr) {
            return index;
        }
        if self.peers.len() >= MAX_PEERS
            && let Some(oldest) = self
                .peers
                .iter()
                .enumerate()
                .min_by_key(|(_, peer)| (peer.live.is_some(), peer.last_seen.mono))
                .map(|(index, _)| index)
        {
            self.peers.swap_remove(oldest);
        }
        self.peers.push(Peer {
            link: ControlLink::new(addr),
            last_seen: now,
            live: None,
        });
        self.peers.len() - 1
    }

    fn on_sealed(
        &mut self,
        tx: &mut TxLane,
        index: usize,
        msg: ControlMessage,
        now: Now,
    ) -> Vec<EndpointEvent> {
        let peer = &mut self.peers[index];
        let Some(live) = &mut peer.live else {
            return Vec::new();
        };
        live.last_valid = now;
        let mut events = Vec::new();
        match msg {
            ControlMessage::Ping { n } => {
                if !live.committed {
                    live.committed = true;
                    log::info!("session {} with {} started", live.id.get(), live.device);
                    events.push(EndpointEvent::SessionStarted {
                        session: live.id,
                        device: live.device,
                        peer: peer.link.peer(),
                        keys: live.keys.clone(),
                        mode: live.mode,
                        codec: live.codec,
                    });
                }
                if let Err(error) = peer.link.send_sealed(tx, &ControlMessage::Pong { n }) {
                    log::debug!("pong not sent: {error}");
                }
            }
            ControlMessage::Bye { .. } => events.extend(peer.end()),
            _ => {}
        }
        events
    }

    fn on_plain(
        &mut self,
        tx: &mut TxLane,
        index: usize,
        msg: &ControlMessage,
        now: Now,
    ) -> Vec<EndpointEvent> {
        let mut events = Vec::new();
        let reply = match msg {
            ControlMessage::SessionInit {
                proto,
                device,
                host,
                noise,
            } => {
                let init = Init {
                    proto: *proto,
                    device: *device,
                    host: *host,
                    noise: &noise.0,
                };
                self.on_init(index, &init, now, &mut events)
            }
            _ => match self.pairing.handle(msg, now, random_pin) {
                Some(event) => {
                    if matches!(event.reply, ControlMessage::PairPrompt { .. }) {
                        self.prompt_peer = Some(self.peers[index].link.peer());
                    }
                    if let Some(paired) = event.paired {
                        self.sessions.add_paired(paired.clone());
                        events.push(EndpointEvent::Paired(paired));
                    }
                    Some(event.reply)
                }
                None => None,
            },
        };
        if let Some(reply) = reply
            && let Err(error) = self.peers[index].link.send(tx, &reply)
        {
            log::debug!("control reply not sent: {error}");
        }
        events
    }

    fn on_init(
        &mut self,
        index: usize,
        init: &Init<'_>,
        now: Now,
        events: &mut Vec<EndpointEvent>,
    ) -> Option<ControlMessage> {
        let addr = self.peers[index].link.peer();
        let busy = self.peers.iter().any(|peer| {
            peer.link.peer() != addr
                && peer
                    .live
                    .as_ref()
                    .is_some_and(|live| live.committed && live.device != init.device)
        });
        if busy {
            return Some(ControlMessage::Reject {
                reason: RejectReason::Busy,
            });
        }
        let session = match random_session() {
            Ok(session) => session,
            Err(error) => {
                log::error!("no session id: {error}");
                return None;
            }
        };
        let policy = self.policy;
        match self
            .sessions
            .on_init(addr, init, session, |hello, id| policy.welcome(hello, id))
        {
            InitOutcome::Ignore => None,
            InitOutcome::Reply(reply) => Some(reply),
            InitOutcome::Accepted {
                reply,
                session,
                device,
                keys,
                mode,
                codec,
            } => {
                // A new session replaces this device's old one, on any address.
                for peer in &mut self.peers {
                    if peer.link.peer() == addr
                        || peer.live.as_ref().is_some_and(|live| live.device == device)
                    {
                        events.extend(peer.end());
                    }
                }
                let peer = &mut self.peers[index];
                peer.link.set_keys(&keys, Role::Host);
                peer.live = Some(Live {
                    id: session,
                    device,
                    committed: false,
                    last_valid: now,
                    keys,
                    mode,
                    codec,
                });
                Some(reply)
            }
        }
    }
}

#[cfg(test)]
#[path = "endpoint_tests.rs"]
mod tests;
