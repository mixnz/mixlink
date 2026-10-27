//! Datagram sealing (spec 0018): AES-128-GCM, header as AAD, tag appended, payload encrypted in place.
//! Nonce = `[channel, 0, 0, 0] ‖ roc ‖ seq` (LE); `roc` counts seq wraps per channel and direction.
//! Keys are fresh per session and direction, so the nonce never repeats while `seq` only moves forward.

use std::fmt;

use aes_gcm::aead::array::Array;
use aes_gcm::{AeadInOut, Aes128Gcm, KeyInit, Nonce, Tag};
use mixlink_core::ids::Seq;

use crate::channel::Channel;
use crate::error::CryptoError;
use crate::header::HEADER_LEN;
use crate::replay::ReplayWindow;
pub use crate::replay::WINDOW;

pub const TAG_LEN: usize = 16;
pub const KEY_LEN: usize = 16;

/// Per-direction session keys: client-to-host and host-to-client.
#[derive(Clone, PartialEq, Eq)]
pub struct SessionKeys {
    pub c2h: [u8; KEY_LEN],
    pub h2c: [u8; KEY_LEN],
}

impl fmt::Debug for SessionKeys {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SessionKeys { .. }")
    }
}

/// Which end of the session this side is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Client,
    Host,
}

impl SessionKeys {
    pub fn sealer(&self, role: Role, channel: Channel) -> Sealer {
        let key = match role {
            Role::Client => &self.c2h,
            Role::Host => &self.h2c,
        };
        Sealer {
            cipher: cipher(key),
            channel,
            last: None,
            roc: 0,
        }
    }

    pub fn opener(&self, role: Role, channel: Channel) -> Opener {
        let key = match role {
            Role::Client => &self.h2c,
            Role::Host => &self.c2h,
        };
        Opener {
            cipher: cipher(key),
            channel,
            replay: ReplayWindow::new(),
            stats: OpenStats::default(),
        }
    }
}

fn cipher(key: &[u8; KEY_LEN]) -> Aes128Gcm {
    Aes128Gcm::new(&Array::from(*key))
}

fn nonce(channel: Channel, roc: u32, seq: u32) -> Nonce<aes_gcm::aead::consts::U12> {
    let mut bytes = [0u8; 12];
    bytes[0] = channel.id();
    bytes[4..8].copy_from_slice(&roc.to_le_bytes());
    bytes[8..12].copy_from_slice(&seq.to_le_bytes());
    Array::from(bytes)
}

/// Channel byte and seq straight from the raw header (offsets per ADR-0008).
fn header_fields(buf: &[u8]) -> (u8, u32) {
    let seq = u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]);
    (buf[1], seq)
}

/// Seals datagrams of one channel; owned by the sending thread.
pub struct Sealer {
    cipher: Aes128Gcm,
    channel: Channel,
    last: Option<u32>,
    roc: u32,
}

impl fmt::Debug for Sealer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Sealer")
            .field("channel", &self.channel)
            .field("roc", &self.roc)
            .finish_non_exhaustive()
    }
}

impl Sealer {
    /// `buf[..len]` is a v1 datagram; encrypts its payload in place and appends the tag.
    /// Returns the sealed length (`len + TAG_LEN`).
    // HOT PATH
    pub fn seal(&mut self, buf: &mut [u8], len: usize) -> Result<usize, CryptoError> {
        if len < HEADER_LEN {
            return Err(CryptoError::TooShort);
        }
        let total = len + TAG_LEN;
        if buf.len() < total {
            return Err(CryptoError::NoRoom);
        }
        let (channel, seq) = header_fields(buf);
        if channel != self.channel.id() {
            return Err(CryptoError::WrongChannel);
        }
        let roc = match self.last {
            None => self.roc,
            Some(last) if !Seq::new(seq).is_newer_than(Seq::new(last)) => {
                return Err(CryptoError::SeqNotNewer);
            }
            Some(last) if seq < last => self.roc.checked_add(1).ok_or(CryptoError::Exhausted)?,
            Some(_) => self.roc,
        };
        let (head, rest) = buf.split_at_mut(HEADER_LEN);
        let (payload, tail) = rest.split_at_mut(len - HEADER_LEN);
        let tag = self
            .cipher
            .encrypt_inout_detached(&nonce(self.channel, roc, seq), head, payload.into())
            .map_err(|_| CryptoError::NoRoom)?;
        tail[..TAG_LEN].copy_from_slice(&tag);
        self.last = Some(seq);
        self.roc = roc;
        Ok(total)
    }
}

/// Counters of one `Opener`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OpenStats {
    pub opened: u64,
    pub bad_tag: u64,
    pub replayed: u64,
    pub too_old: u64,
}

/// Opens datagrams of one channel; owned by the receiving thread.
pub struct Opener {
    cipher: Aes128Gcm,
    channel: Channel,
    replay: ReplayWindow,
    stats: OpenStats,
}

impl fmt::Debug for Opener {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Opener")
            .field("channel", &self.channel)
            .field("stats", &self.stats)
            .finish_non_exhaustive()
    }
}

impl Opener {
    /// `buf` is one sealed datagram; decrypts in place and returns the plaintext datagram length.
    // HOT PATH
    pub fn open(&mut self, buf: &mut [u8]) -> Result<usize, CryptoError> {
        if buf.len() < HEADER_LEN + TAG_LEN {
            return Err(CryptoError::TooShort);
        }
        let (channel, seq) = header_fields(buf);
        if channel != self.channel.id() {
            return Err(CryptoError::WrongChannel);
        }
        let index = match self.replay.check(seq) {
            Ok(index) => index,
            Err(error) => {
                match error {
                    CryptoError::Replayed => self.stats.replayed += 1,
                    _ => self.stats.too_old += 1,
                }
                return Err(error);
            }
        };
        let len = buf.len() - TAG_LEN;
        let (head, rest) = buf.split_at_mut(HEADER_LEN);
        let (payload, tag) = rest.split_at_mut(len - HEADER_LEN);
        let tag = Tag::<aes_gcm::aead::consts::U16>::try_from(&tag[..])
            .map_err(|_| CryptoError::TooShort)?;
        let roc = (index >> 32) as u32;
        if self
            .cipher
            .decrypt_inout_detached(&nonce(self.channel, roc, seq), head, payload.into(), &tag)
            .is_err()
        {
            self.stats.bad_tag += 1;
            return Err(CryptoError::BadTag);
        }
        self.replay.accept(index);
        self.stats.opened += 1;
        Ok(len)
    }

    pub fn stats(&self) -> OpenStats {
        self.stats
    }
}

#[cfg(test)]
#[path = "crypto_tests.rs"]
mod tests;
