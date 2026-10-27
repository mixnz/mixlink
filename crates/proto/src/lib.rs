//! Wire format: packet header, channel ids, (de)serialization, FEC encode/recover, AES-GCM framing. Pure logic, no sockets.

pub mod blob;
pub mod channel;
pub mod control;
pub mod crypto;
pub mod cursor;
pub mod error;
pub mod feedback;
pub mod header;
pub mod packetizer;
pub mod reassembler;
pub mod video;

mod replay;
mod slot;
#[cfg(test)]
mod test_util;
