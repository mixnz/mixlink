//! Client side of the T3 handshake: send `Hello` on the control lane until `Welcome` / `Reject`.
//! Control plane (blocking, allocation ok); runs on the client's control thread.

use std::net::SocketAddrV4;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use mixlink_core::clock;
use mixlink_core::ids::{FrameId, SessionId};
use mixlink_proto::channel::Channel;
use mixlink_proto::control::{self, ControlMessage, DisplayInfo, RejectReason};
use mixlink_proto::header::{Flags, PacketHeader};
use mixlink_proto::packetizer::Packetizer;
use mixlink_proto::reassembler::{Push, Reassembler, ReassemblerConfig};

use crate::error::NetError;
use crate::lane::{RxLane, TxLane};
use crate::resend::{HELLO_GIVE_UP, HELLO_RESEND, ResendAction, ResendTimer};

/// Longest single wait, so `cancel` is seen promptly.
const CANCEL_POLL: Duration = Duration::from_millis(50);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HandshakeReply {
    Welcome {
        session: SessionId,
        mode: DisplayInfo,
    },
    Reject(RejectReason),
}

/// Sends `hello` to `host` every 250 ms until it answers, for at most 10 s.
pub fn connect(
    tx: &mut TxLane,
    rx: &mut RxLane,
    host: SocketAddrV4,
    hello: &ControlMessage,
    cancel: &AtomicBool,
) -> Result<HandshakeReply, NetError> {
    connect_with(tx, rx, host, hello, cancel, HELLO_RESEND, HELLO_GIVE_UP)
}

/// `connect` with explicit timing. Only datagrams from `host` count; each resend uses a new
/// `FrameId` (the host drops a repeated one as late).
pub fn connect_with(
    tx: &mut TxLane,
    rx: &mut RxLane,
    host: SocketAddrV4,
    hello: &ControlMessage,
    cancel: &AtomicBool,
    interval: Duration,
    give_up: Duration,
) -> Result<HandshakeReply, NetError> {
    let message = control::encode(hello)?;
    let mut packetizer = Packetizer::new(Channel::Control);
    let mut reassembler = Reassembler::new(ReassemblerConfig::for_channel(Channel::Control));
    let mut timer = ResendTimer::new(clock::now(), interval, give_up);
    let mut frame = FrameId::new(0);
    loop {
        if cancel.load(Ordering::Acquire) {
            return Err(NetError::Cancelled);
        }
        match timer.poll(clock::now()) {
            ResendAction::GiveUp => return Err(NetError::Timeout),
            ResendAction::Send => {
                send_message(tx, &mut packetizer, frame, &message, host)?;
                frame = frame.next();
            }
            ResendAction::Wait(wait) => {
                if let Some(reply) = receive(rx, &mut reassembler, host, wait.min(CANCEL_POLL)) {
                    return reply;
                }
            }
        }
    }
}

/// Queues every fragment; a full lane drops this round (the next `Send` retries).
fn send_message(
    tx: &mut TxLane,
    packetizer: &mut Packetizer,
    frame: FrameId,
    message: &[u8],
    host: SocketAddrV4,
) -> Result<(), NetError> {
    let mut fragments = packetizer.packetize(frame, Flags::empty(), message)?;
    while let Some(mut slot) = tx.try_reserve() {
        let Some(len) = fragments.next_into(slot.datagram()) else {
            break;
        };
        slot.commit(len, host);
    }
    Ok(())
}

/// Waits up to `wait` for one datagram; `Some` once a handshake answer is complete.
fn receive(
    rx: &mut RxLane,
    reassembler: &mut Reassembler,
    host: SocketAddrV4,
    wait: Duration,
) -> Option<Result<HandshakeReply, NetError>> {
    let datagram = rx.recv_timeout(wait)?;
    if datagram.peer() != host {
        return None;
    }
    let (header, payload) = PacketHeader::decode(datagram.data()).ok()?;
    let Push::Complete(message) = reassembler.push(&header, payload) else {
        return None;
    };
    match control::decode(message.data).ok()? {
        ControlMessage::Welcome { session, mode } => {
            Some(Ok(HandshakeReply::Welcome { session, mode }))
        }
        ControlMessage::Reject { reason } => Some(Ok(HandshakeReply::Reject(reason))),
        ControlMessage::Bye { .. } => Some(Err(NetError::UnexpectedReply)),
        ControlMessage::Hello { .. } => None,
    }
}

#[cfg(test)]
#[path = "handshake_tests.rs"]
mod tests;
