//! Typed errors for net modules. Hot-path failures are counters (`stats`), not errors.

use std::io;
use std::net::SocketAddrV4;

use mixlink_proto::error::{ControlError, PacketizeError};

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
    #[error("unexpected reply from the host")]
    UnexpectedReply,
}
