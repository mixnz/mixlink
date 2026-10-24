//! Message -> datagrams, pulled one at a time by the sender (a pacer spreads them out).

use mixlink_core::ids::{FrameId, Seq};

use crate::channel::Channel;
use crate::error::PacketizeError;
use crate::header::{Flags, HEADER_LEN, MAX_DATAGRAM, MAX_PAYLOAD, PacketHeader};

/// Datagrams needed for a message of `len` bytes; `None` above `u16::MAX` fragments.
pub fn fragment_count(len: usize) -> Option<u16> {
    let count = len.div_ceil(MAX_PAYLOAD).max(1);
    u16::try_from(count).ok()
}

/// Splits messages of one channel and direction into datagrams; owns that channel's `Seq`.
#[derive(Debug)]
pub struct Packetizer {
    channel: Channel,
    next_seq: Seq,
}

impl Packetizer {
    pub const fn new(channel: Channel) -> Self {
        Self::with_seq(channel, Seq::new(0))
    }

    pub const fn with_seq(channel: Channel, next: Seq) -> Self {
        Self {
            channel,
            next_seq: next,
        }
    }

    /// `Seq` the next datagram will carry.
    pub const fn next_seq(&self) -> Seq {
        self.next_seq
    }

    pub fn packetize<'p, 'm>(
        &'p mut self,
        frame: FrameId,
        flags: Flags,
        message: &'m [u8],
    ) -> Result<Fragments<'p, 'm>, PacketizeError> {
        let count =
            fragment_count(message.len()).ok_or(PacketizeError::TooLarge { len: message.len() })?;
        Ok(Fragments {
            packetizer: self,
            message,
            frame,
            flags,
            next_index: 0,
            count,
        })
    }
}

/// The datagrams of one message, written on demand.
#[derive(Debug)]
pub struct Fragments<'p, 'm> {
    packetizer: &'p mut Packetizer,
    message: &'m [u8],
    frame: FrameId,
    flags: Flags,
    next_index: u16,
    count: u16,
}

impl Fragments<'_, '_> {
    pub const fn count(&self) -> u16 {
        self.count
    }

    /// Writes the next datagram into `out` and returns its length; `None` when all are written.
    // HOT PATH
    pub fn next_into(&mut self, out: &mut [u8; MAX_DATAGRAM]) -> Option<usize> {
        if self.next_index >= self.count {
            return None;
        }
        let start = usize::from(self.next_index) * MAX_PAYLOAD;
        let end = (start + MAX_PAYLOAD).min(self.message.len());
        let chunk = self.message.get(start..end).unwrap_or_default();
        let header = PacketHeader {
            channel: self.packetizer.channel,
            flags: self.flags,
            seq: self.packetizer.next_seq,
            frame: self.frame,
            frag_index: self.next_index,
            frag_count: self.count,
        };
        let (head, body) = out.split_first_chunk_mut::<HEADER_LEN>()?;
        header.encode(head);
        body[..chunk.len()].copy_from_slice(chunk);
        self.packetizer.next_seq = self.packetizer.next_seq.next();
        self.next_index += 1;
        Some(HEADER_LEN + chunk.len())
    }
}

#[cfg(test)]
#[path = "packetizer_tests.rs"]
mod tests;
