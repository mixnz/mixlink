//! Host control endpoint (spec 0018): pairing and session handshakes for every peer on the
//! session port's control lane. The service's `svc-session` thread drives it. Control plane.

use std::net::SocketAddrV4;
use std::time::Duration;

use mixlink_core::ids::{DeviceId, SessionId};
use mixlink_proto::control::{ControlMessage, RejectReason};
use mixlink_proto::crypto::Role;

use crate::control_link::ControlLink;
use crate::keys::{StaticKeyPair, random_pin, random_session};
use crate::lane::{RxDatagram, TxLane};
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EndpointEvent {
    Paired(PairedDevice),
    /// The client proved its keys with a sealed datagram.
    SessionStarted {
        session: SessionId,
        device: DeviceId,
        peer: SocketAddrV4,
    },
    SessionEnded {
        session: SessionId,
        device: DeviceId,
    },
}

#[derive(Debug)]
struct Live {
    id: SessionId,
    device: DeviceId,
    committed: bool,
    last_valid: Now,
}

#[derive(Debug)]
struct Peer {
    link: ControlLink,
    last_seen: Now,
    live: Option<Live>,
}

impl Peer {
    /// Drops the session keys; an event when the session had started.
    fn end(&mut self) -> Option<EndpointEvent> {
        self.link.clear_keys();
        let live = self.live.take()?;
        log::info!("session {} with {} ended", live.id.get(), live.device);
        live.committed.then_some(EndpointEvent::SessionEnded {
            session: live.id,
            device: live.device,
        })
    }
}

#[derive(Debug)]
pub struct HostControl {
    pairing: HostPairing,
    sessions: HostSessions,
    peers: Vec<Peer>,
}

impl HostControl {
    pub fn new(config: HostControlConfig) -> HostControl {
        HostControl {
            pairing: HostPairing::new(config.host, config.keys.clone()),
            sessions: HostSessions::new(config.host, config.keys, config.paired),
            peers: Vec::new(),
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

    pub fn cancel_prompt(&mut self) {
        self.pairing.cancel();
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
        match self.sessions.on_init(addr, init, session, welcome) {
            InitOutcome::Ignore => None,
            InitOutcome::Reply(reply) => Some(reply),
            InitOutcome::Accepted {
                reply,
                session,
                device,
                keys,
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
                });
                Some(reply)
            }
        }
    }
}

/// Streams the client's own display mode back. The real mode choice (virtual monitor) comes with
/// streaming; spec 0018 only proves the session.
fn welcome(hello: &ControlMessage, session: SessionId) -> ControlMessage {
    match hello {
        ControlMessage::Hello {
            displays,
            stream_display,
            ..
        } => match displays.get(usize::from(*stream_display)) {
            Some(mode) => ControlMessage::Welcome {
                session,
                mode: *mode,
            },
            None => ControlMessage::Reject {
                reason: RejectReason::BadDisplay,
            },
        },
        _ => ControlMessage::Reject {
            reason: RejectReason::BadDisplay,
        },
    }
}

#[cfg(test)]
#[path = "endpoint_tests.rs"]
mod tests;
