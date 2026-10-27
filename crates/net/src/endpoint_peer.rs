//! One peer of `HostControl` (spec 0018, 0020): its control link and, once the handshake is answered,
//! its session. Control plane.

use mixlink_core::ids::{DeviceId, SessionId};
use mixlink_proto::control::{ByeReason, CodecId, ControlMessage, DisplayInfo};
use mixlink_proto::crypto::SessionKeys;

use crate::control_link::ControlLink;
use crate::endpoint::EndpointEvent;
use crate::lane::TxLane;
use crate::pairing::Now;

/// Copies of the close notice sent on cancel (the client also learns on its next PIN) and of a host
/// `Bye`: one lost datagram must not leave the client waiting.
pub(crate) const CLOSE_NOTICES: usize = 3;

#[derive(Debug)]
pub(crate) struct Live {
    pub(crate) id: SessionId,
    pub(crate) device: DeviceId,
    pub(crate) committed: bool,
    pub(crate) last_valid: Now,
    pub(crate) keys: SessionKeys,
    pub(crate) mode: DisplayInfo,
    pub(crate) codec: CodecId,
}

#[derive(Debug)]
pub(crate) struct Peer {
    pub(crate) link: ControlLink,
    pub(crate) last_seen: Now,
    pub(crate) live: Option<Live>,
}

impl Peer {
    /// Drops the session keys; an event when the session had started.
    pub(crate) fn end(&mut self) -> Option<EndpointEvent> {
        self.link.clear_keys();
        let live = self.live.take()?;
        log::info!("session {} with {} ended", live.id.get(), live.device);
        live.committed.then_some(EndpointEvent::SessionEnded {
            session: live.id,
            device: live.device,
        })
    }

    /// Sends `Bye { reason }` sealed, `CLOSE_NOTICES` times; nothing without session keys.
    pub(crate) fn say_bye(&mut self, tx: &mut TxLane, reason: ByeReason) {
        if !self.link.has_keys() {
            return;
        }
        for _ in 0..CLOSE_NOTICES {
            if let Err(error) = self.link.send_sealed(tx, &ControlMessage::Bye { reason }) {
                log::debug!("bye not sent: {error}");
            }
        }
    }
}
