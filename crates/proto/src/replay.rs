//! Receive-side seq tracking for `crypto::Opener` (spec 0018): rollover guess and replay window.

use crate::error::CryptoError;

/// Replay window in datagrams, per channel.
pub const WINDOW: usize = 1024;
const WINDOW_WORDS: usize = WINDOW / 64;

#[derive(Debug, Clone)]
pub(crate) struct ReplayWindow {
    /// Highest accepted index (`roc << 32 | seq`).
    highest: Option<u64>,
    /// Bit `k` = index `highest - k` was accepted.
    bits: [u64; WINDOW_WORDS],
}

impl ReplayWindow {
    pub(crate) fn new() -> ReplayWindow {
        ReplayWindow {
            highest: None,
            bits: [0; WINDOW_WORDS],
        }
    }

    /// The index for `seq` closest to the highest accepted one (RFC 3711 3.3.1), if not seen
    /// and not older than the window. Nothing changes until `accept`.
    pub(crate) fn check(&self, seq: u32) -> Result<u64, CryptoError> {
        let index = self.estimate(seq).ok_or(CryptoError::TooOld)?;
        if let Some(highest) = self.highest
            && index <= highest
        {
            let back = highest - index;
            if back >= WINDOW as u64 {
                return Err(CryptoError::TooOld);
            }
            if self.bit(back) {
                return Err(CryptoError::Replayed);
            }
        }
        Ok(index)
    }

    /// Marks `index` (from `check`) as received; call only after the tag checked out.
    pub(crate) fn accept(&mut self, index: u64) {
        match self.highest {
            Some(highest) if index <= highest => {
                let back = (highest - index) as usize;
                self.bits[back / 64] |= 1 << (back % 64);
            }
            Some(highest) => {
                self.shift(index - highest);
                self.bits[0] |= 1;
                self.highest = Some(index);
            }
            None => {
                self.bits = [0; WINDOW_WORDS];
                self.bits[0] = 1;
                self.highest = Some(index);
            }
        }
    }

    /// `None` = before roc 0. The first datagram of a session is taken as roc 0.
    fn estimate(&self, seq: u32) -> Option<u64> {
        const HALF: u32 = 1 << 31;
        let Some(highest) = self.highest else {
            return Some(u64::from(seq));
        };
        let roc = highest >> 32;
        let last = highest as u32;
        let roc = if last < HALF {
            if seq > last && seq - last > HALF {
                roc.checked_sub(1)?
            } else {
                roc
            }
        } else if seq < last - HALF {
            roc + 1
        } else {
            roc
        };
        Some((roc << 32) | u64::from(seq))
    }

    fn bit(&self, back: u64) -> bool {
        let back = back as usize;
        self.bits[back / 64] & (1 << (back % 64)) != 0
    }

    /// Moves every bit `by` positions older; bits past the window fall off.
    fn shift(&mut self, by: u64) {
        if by >= WINDOW as u64 {
            self.bits = [0; WINDOW_WORDS];
            return;
        }
        let words = (by / 64) as usize;
        let bits = (by % 64) as u32;
        for i in (0..WINDOW_WORDS).rev() {
            let src = i.checked_sub(words);
            let hi = src.map_or(0, |s| self.bits[s]);
            let lo = src
                .and_then(|s| s.checked_sub(1))
                .map_or(0, |s| self.bits[s]);
            self.bits[i] = if bits == 0 {
                hi
            } else {
                (hi << bits) | (lo >> (64 - bits))
            };
        }
    }
}
