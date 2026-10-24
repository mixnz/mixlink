//! UDP socket setup, send pacer, bitrate/congestion controller, LAN discovery (mDNS + broadcast), pairing handshake. Later: ICE/STUN/TURN.

pub mod error;
pub mod lane;
pub mod socket;
pub mod stats;

mod platform;

pub use lane::{RxDatagram, RxLane, TxLane, TxSlot};
