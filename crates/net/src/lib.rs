//! UDP socket setup, send pacer, bitrate/congestion controller, LAN discovery (mDNS + broadcast), pairing handshake. Later: ICE/STUN/TURN.

pub mod client_store;
pub mod control_link;
pub mod discovery;
pub mod endpoint;
pub mod error;
pub mod handshake;
pub mod keys;
pub mod lane;
pub mod pairing;
pub mod resend;
pub mod session;
pub mod socket;
pub mod stats;
pub mod transport;

mod platform;
mod recv_loop;
mod send_loop;

pub use lane::{RxDatagram, RxLane, TxLane, TxSlot};
pub use stats::TransportStats;
pub use transport::{LaneCapacities, Lanes, Transport, TransportConfig};
