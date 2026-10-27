//! Session handshake (spec 0018): Noise `KK` with stored static keys, then sealed datagrams.

pub mod client;
pub mod host;
pub mod noise;

/// The client sends a sealed `Ping` this often; the host's `Pong` keeps the session alive.
pub const PING_EVERY: std::time::Duration = std::time::Duration::from_secs(1);
/// No valid datagram for this long ends the session on both sides.
pub const SESSION_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
