//! Wire format: packet header, channel ids, (de)serialization, FEC encode/recover, AES-GCM framing. Pure logic, no sockets.

pub mod channel;
pub mod control;
pub mod crypto;
pub mod cursor;
pub mod error;
pub mod header;
pub mod packetizer;
pub mod reassembler;

mod replay;
mod slot;
#[cfg(test)]
mod test_util;
