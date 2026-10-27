//! Typed errors for net modules. Hot-path failures are counters (`stats`), not errors.

use std::io;
use std::net::SocketAddrV4;

use mixlink_proto::control::RejectReason;
use mixlink_proto::error::{ControlError, CryptoError, PacketizeError};

#[derive(Debug, thiserror::Error)]
pub enum NetError {
    #[error("cannot bind UDP socket to {addr}: {source}")]
    Bind {
        addr: SocketAddrV4,
        source: io::Error,
    },
    #[error("cannot set socket option {option}: {source}")]
    SocketOption {
        option: &'static str,
        source: io::Error,
    },
    #[error("cannot start network thread: {0}")]
    Spawn(io::Error),
    #[error("no reply from the host in time")]
    Timeout,
    #[error("cancelled")]
    Cancelled,
    #[error(transparent)]
    Control(#[from] ControlError),
    #[error(transparent)]
    Packetize(#[from] PacketizeError),
    #[error(transparent)]
    Crypto(#[from] CryptoError),
    #[error("key or random source failed: {0}")]
    Keys(String),
    #[error("the host refused: {0:?}")]
    Rejected(RejectReason),
    #[error("session handshake failed: {0}")]
    Noise(String),
    #[error("no session keys yet")]
    NoSession,
    #[error("unexpected reply from the host")]
    UnexpectedReply,
}
