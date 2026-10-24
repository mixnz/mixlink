//! Packet header v1 (ADR-0008): 16 bytes, little-endian, in front of every datagram payload.

use std::ops::BitOr;

use mixlink_core::ids::{FrameId, Seq};

use crate::channel::Channel;
use crate::error::DecodeError;

pub const WIRE_VERSION: u8 = 1;
pub const HEADER_LEN: usize = 16;
/// Payload bytes per datagram; with header and AES-GCM tag the UDP payload stays ≤ 1232.
pub const MAX_PAYLOAD: usize = 1200;
/// Largest datagram before encryption.
pub const MAX_DATAGRAM: usize = HEADER_LEN + MAX_PAYLOAD;

/// Header flag bits; unknown bits make a packet invalid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Flags(u16);

impl Flags {
    pub const KEYFRAME: Flags = Flags(1 << 0);
    /// FEC parity shard; reserved until FEC lands.
    pub const PARITY: Flags = Flags(1 << 1);
    pub const KNOWN: Flags = Flags(Self::KEYFRAME.0 | Self::PARITY.0);

    pub const fn empty() -> Self {
        Flags(0)
    }

    pub const fn bits(self) -> u16 {
        self.0
    }

    pub const fn from_bits(bits: u16) -> Option<Flags> {
        if bits & !Self::KNOWN.0 == 0 {
            Some(Flags(bits))
        } else {
            None
        }
    }

    pub const fn contains(self, other: Flags) -> bool {
        self.0 & other.0 == other.0
    }
}

impl BitOr for Flags {
    type Output = Flags;

    fn bitor(self, rhs: Flags) -> Flags {
        Flags(self.0 | rhs.0)
    }
}

/// Decoded header; the version byte is implicit (`WIRE_VERSION`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PacketHeader {
    pub channel: Channel,
    pub flags: Flags,
    /// Per channel and direction, +1 per datagram.
    pub seq: Seq,
    /// Frame number (video / audio) or message number (other channels).
    pub frame: FrameId,
    pub frag_index: u16,
    pub frag_count: u16,
}

impl PacketHeader {
    // HOT PATH
    pub fn encode(&self, out: &mut [u8; HEADER_LEN]) {
        out[0] = WIRE_VERSION;
        out[1] = self.channel.id();
        out[2..4].copy_from_slice(&self.flags.bits().to_le_bytes());
        out[4..8].copy_from_slice(&self.seq.get().to_le_bytes());
        out[8..12].copy_from_slice(&self.frame.get().to_le_bytes());
        out[12..14].copy_from_slice(&self.frag_index.to_le_bytes());
        out[14..16].copy_from_slice(&self.frag_count.to_le_bytes());
    }

    /// Splits a datagram into its header and payload, checking every v1 rule.
    // HOT PATH
    pub fn decode(datagram: &[u8]) -> Result<(PacketHeader, &[u8]), DecodeError> {
        let Some((head, payload)) = datagram.split_first_chunk::<HEADER_LEN>() else {
            return Err(DecodeError::TooShort {
                len: datagram.len(),
            });
        };
        if head[0] != WIRE_VERSION {
            return Err(DecodeError::Version { found: head[0] });
        }
        let channel = Channel::from_id(head[1]).ok_or(DecodeError::Channel { id: head[1] })?;
        let bits = u16::from_le_bytes([head[2], head[3]]);
        let flags = Flags::from_bits(bits).ok_or(DecodeError::Flags { bits })?;
        let seq = u32::from_le_bytes([head[4], head[5], head[6], head[7]]);
        let frame = u32::from_le_bytes([head[8], head[9], head[10], head[11]]);
        let frag_index = u16::from_le_bytes([head[12], head[13]]);
        let frag_count = u16::from_le_bytes([head[14], head[15]]);
        if frag_count == 0 {
            return Err(DecodeError::FragCount);
        }
        if frag_index >= frag_count {
            return Err(DecodeError::FragIndex {
                index: frag_index,
                count: frag_count,
            });
        }
        if payload.len() > MAX_PAYLOAD {
            return Err(DecodeError::PayloadTooLarge { len: payload.len() });
        }
        if frag_index + 1 < frag_count && payload.len() != MAX_PAYLOAD {
            return Err(DecodeError::ShortFragment {
                index: frag_index,
                len: payload.len(),
            });
        }
        let header = PacketHeader {
            channel,
            flags,
            seq: Seq::new(seq),
            frame: FrameId::new(frame),
            frag_index,
            frag_count,
        };
        Ok((header, payload))
    }
}

#[cfg(test)]
#[path = "header_tests.rs"]
mod tests;
