//! Helpers shared by the unit tests of several modules.

use mixlink_core::ids::FrameId;

use crate::header::{Flags, MAX_DATAGRAM, PacketHeader};
use crate::packetizer::Packetizer;

/// Deterministic xorshift64; no `rand` dependency.
pub(crate) struct Rng(pub(crate) u64);

impl Rng {
    pub(crate) fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    pub(crate) fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    pub(crate) fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            items.swap(i, self.below(i + 1));
        }
    }
}

pub(crate) type Datagram = (PacketHeader, Vec<u8>);

pub(crate) fn datagrams(
    p: &mut Packetizer,
    frame: u32,
    flags: Flags,
    message: &[u8],
) -> Vec<Datagram> {
    let mut fragments = p.packetize(FrameId::new(frame), flags, message).unwrap();
    let mut buf = [0u8; MAX_DATAGRAM];
    let mut out = Vec::new();
    while let Some(len) = fragments.next_into(&mut buf) {
        let (h, body) = PacketHeader::decode(&buf[..len]).unwrap();
        out.push((h, body.to_vec()));
    }
    out
}
