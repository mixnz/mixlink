//! UDP socket setup, send pacer, bitrate/congestion controller, LAN discovery (mDNS + broadcast), pairing handshake. Later: ICE/STUN/TURN.

pub mod error;
pub mod socket;

mod platform;
