//! One peer's control channel (spec 0018): JSON messages over plaintext or sealed datagrams.
//! Control plane (allocation ok). The same `ControlLink` must carry every message to one peer:
//! its frame numbers only move forward, so a fresh link to a peer's old link would be dropped as late.

use std::net::SocketAddrV4;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use mixlink_core::clock;
use mixlink_core::ids::FrameId;
use mixlink_proto::channel::Channel;
use mixlink_proto::control::{self, ControlMessage};
use mixlink_proto::crypto::{Opener, Role, Sealer, SessionKeys};
use mixlink_proto::header::{Flags, PacketHeader};
use mixlink_proto::packetizer::Packetizer;
use mixlink_proto::reassembler::{Push, Reassembler, ReassemblerConfig};

use crate::error::NetError;
use crate::lane::{RxDatagram, RxLane, TxLane};
use crate::resend::{ResendAction, ResendTimer};

/// Longest single wait in `exchange`, so `cancel` is seen promptly.
const CANCEL_POLL: Duration = Duration::from_millis(50);

/// The lanes and cancel flag a blocking client exchange runs on.
#[derive(Debug)]
pub struct Wire<'a> {
    pub tx: &'a mut TxLane,
    pub rx: &'a mut RxLane,
    pub cancel: &'a AtomicBool,
}

/// A decoded control message and whether it came sealed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Incoming {
    pub msg: ControlMessage,
    pub sealed: bool,
}

#[derive(Debug)]
pub struct ControlLink {
    peer: SocketAddrV4,
    packetizer: Packetizer,
    frame: FrameId,
    /// Separate reassemblers, so a datagram that fails to open never disturbs sealed reassembly.
    plain: Reassembler,
    sealed: Reassembler,
    crypto: Option<(Sealer, Opener)>,
}

impl ControlLink {
    pub fn new(peer: SocketAddrV4) -> ControlLink {
        let config = ReassemblerConfig::for_channel(Channel::Control);
        ControlLink {
            peer,
            packetizer: Packetizer::new(Channel::Control),
            frame: FrameId::new(0),
            plain: Reassembler::new(config),
            sealed: Reassembler::new(config),
            crypto: None,
        }
    }

    pub fn peer(&self) -> SocketAddrV4 {
        self.peer
    }

    /// From now on `send_sealed` works and sealed datagrams are opened.
    pub fn set_keys(&mut self, keys: &SessionKeys, role: Role) {
        self.crypto = Some((
            keys.sealer(role, Channel::Control),
            keys.opener(role, Channel::Control),
        ));
    }

    pub fn clear_keys(&mut self) {
        self.crypto = None;
    }

    pub fn has_keys(&self) -> bool {
        self.crypto.is_some()
    }

    /// Queues `msg` in plaintext. A full lane drops the rest of the message (the caller resends).
    pub fn send(&mut self, tx: &mut TxLane, msg: &ControlMessage) -> Result<(), NetError> {
        self.queue(tx, msg, false)
    }

    /// Queues `msg` sealed with the session keys.
    pub fn send_sealed(&mut self, tx: &mut TxLane, msg: &ControlMessage) -> Result<(), NetError> {
        self.queue(tx, msg, true)
    }

    fn queue(
        &mut self,
        tx: &mut TxLane,
        msg: &ControlMessage,
        sealed: bool,
    ) -> Result<(), NetError> {
        let bytes = control::encode(msg)?;
        let mut fragments = self
            .packetizer
            .packetize(self.frame, Flags::empty(), &bytes)?;
        self.frame = self.frame.next();
        for _ in 0..fragments.count() {
            let Some(mut slot) = tx.try_reserve() else {
                break;
            };
            let Some(len) = fragments.next_into(slot.datagram()) else {
                break;
            };
            match (&mut self.crypto, sealed) {
                (_, false) => slot.commit(len, self.peer),
                (Some((sealer, _)), true) => slot.seal_commit(sealer, len, self.peer)?,
                (None, true) => return Err(NetError::NoSession),
            }
        }
        Ok(())
    }

    /// A complete message from this datagram, if it finishes one. The caller checks the sender.
    /// With keys, a datagram is tried sealed first, then as plaintext (a restarted peer).
    pub fn accept(&mut self, dg: &mut RxDatagram<'_>) -> Option<Incoming> {
        if let Some((_, opener)) = &mut self.crypto
            && let Ok(plain) = dg.open(opener)
        {
            return reassemble(&mut self.sealed, plain).map(|msg| Incoming { msg, sealed: true });
        }
        reassemble(&mut self.plain, dg.data()).map(|msg| Incoming { msg, sealed: false })
    }

    /// Sends `msg` every `timing.0` until `pick` accepts a reply from the peer, for at most `timing.1`.
    pub fn exchange<T>(
        &mut self,
        wire: &mut Wire<'_>,
        msg: &ControlMessage,
        sealed: bool,
        (interval, give_up): (Duration, Duration),
        mut pick: impl FnMut(Incoming) -> Option<T>,
    ) -> Result<T, NetError> {
        let mut timer = ResendTimer::new(clock::now(), interval, give_up);
        loop {
            if wire.cancel.load(Ordering::Acquire) {
                return Err(NetError::Cancelled);
            }
            match timer.poll(clock::now()) {
                ResendAction::GiveUp => return Err(NetError::Timeout),
                ResendAction::Send => self.queue(wire.tx, msg, sealed)?,
                ResendAction::Wait(wait) => {
                    let Some(mut dg) = wire.rx.recv_timeout(wait.min(CANCEL_POLL)) else {
                        continue;
                    };
                    if dg.peer() != self.peer {
                        continue;
                    }
                    if let Some(found) = self.accept(&mut dg).and_then(&mut pick) {
                        return Ok(found);
                    }
                }
            }
        }
    }
}

fn reassemble(reassembler: &mut Reassembler, datagram: &[u8]) -> Option<ControlMessage> {
    let (header, payload) = PacketHeader::decode(datagram).ok()?;
    let Push::Complete(message) = reassembler.push(&header, payload) else {
        return None;
    };
    match control::decode(message.data) {
        Ok(msg) => Some(msg),
        Err(error) => {
            log::debug!("bad control message: {error}");
            None
        }
    }
}
