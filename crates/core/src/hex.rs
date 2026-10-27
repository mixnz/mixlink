//! Lowercase hex text for ids and keys in JSON. Decoding accepts both cases.

use crate::error::HexError;

pub fn encode(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut text = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        text.push(char::from(DIGITS[usize::from(b >> 4)]));
        text.push(char::from(DIGITS[usize::from(b & 0x0f)]));
    }
    text
}

/// Fills `out` from exactly `2 * out.len()` hex digits.
pub fn decode_into(text: &str, out: &mut [u8]) -> Result<(), HexError> {
    let text = text.as_bytes();
    if text.len() != out.len() * 2 {
        return Err(HexError::Length {
            expected: out.len() * 2,
            len: text.len(),
        });
    }
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = (digit(text, 2 * i)? << 4) | digit(text, 2 * i + 1)?;
    }
    Ok(())
}

/// Decodes an even number of digits into at most `max` bytes.
pub fn decode_vec(text: &str, max: usize) -> Result<Vec<u8>, HexError> {
    let len = text.len();
    if !len.is_multiple_of(2) {
        return Err(HexError::OddLength { len });
    }
    if len / 2 > max {
        return Err(HexError::TooLong { max, len: len / 2 });
    }
    let mut out = vec![0u8; len / 2];
    decode_into(text, &mut out)?;
    Ok(out)
}

/// The value of the hex digit at `index`.
pub fn digit(text: &[u8], index: usize) -> Result<u8, HexError> {
    let c = text.get(index).copied().unwrap_or(0);
    match c {
        b'0'..=b'9' => Ok(c - b'0'),
        b'a'..=b'f' => Ok(c - b'a' + 10),
        b'A'..=b'F' => Ok(c - b'A' + 10),
        _ => Err(HexError::NotHex { index }),
    }
}

#[cfg(test)]
#[path = "hex_tests.rs"]
mod tests;
