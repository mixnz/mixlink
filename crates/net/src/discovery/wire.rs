//! Broadcast fallback wire format (spec 0017): probe = magic + version; reply = the same 5 bytes
//! followed by the `Advert` as UTF-8 JSON.

use crate::discovery::advert::Advert;
use crate::discovery::error::DiscoveryError;

pub const MAGIC: [u8; 4] = *b"MXLD";
pub const DISCOVERY_VERSION: u8 = 1;
pub const PROBE: [u8; 5] = [MAGIC[0], MAGIC[1], MAGIC[2], MAGIC[3], DISCOVERY_VERSION];
/// Replies stay within one unfragmented datagram.
pub const MAX_REPLY: usize = 1200;

pub fn is_probe(buf: &[u8]) -> bool {
    buf == PROBE
}

/// Writes the reply for `advert` into `out` (cleared first).
pub fn encode_reply(advert: &Advert, out: &mut Vec<u8>) -> Result<(), DiscoveryError> {
    out.clear();
    out.extend_from_slice(&PROBE);
    serde_json::to_writer(&mut *out, advert)?;
    if out.len() > MAX_REPLY {
        return Err(DiscoveryError::ReplyTooLong { len: out.len() });
    }
    Ok(())
}

/// The advert in a reply; `None` for anything that is not a valid reply.
pub fn decode_reply(buf: &[u8]) -> Option<Advert> {
    if buf.len() > MAX_REPLY {
        return None;
    }
    let body = buf.strip_prefix(&PROBE)?;
    let advert: Advert = serde_json::from_slice(body).ok()?;
    advert.is_valid().then_some(advert)
}

#[cfg(test)]
#[path = "wire_tests.rs"]
mod tests;
