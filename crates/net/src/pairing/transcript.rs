//! Pairing transcript and confirm MACs (spec 0018). Both static keys are bound, so swapping
//! either one makes the confirm fail.

use hkdf::Hkdf;
use hmac::{Hmac, KeyInit, Mac};
use mixlink_core::ids::DeviceId;
use mixlink_proto::control::PublicKey;
use sha2::{Digest, Sha256};

const LABEL: &[u8] = b"mixlink-pair-v1";

/// The values both sides hash; SPAKE2 messages are fixed length (`SPAKE_LEN`).
#[derive(Debug, Clone, Copy)]
pub struct Parts<'a> {
    pub device: &'a DeviceId,
    pub host: &'a DeviceId,
    pub client_key: &'a PublicKey,
    pub host_key: &'a PublicKey,
    pub spake_a: &'a [u8],
    pub spake_b: &'a [u8],
}

pub fn transcript(parts: &Parts<'_>) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(LABEL);
    hash.update(parts.device.as_bytes());
    hash.update(parts.host.as_bytes());
    hash.update(parts.client_key.0);
    hash.update(parts.host_key.0);
    hash.update(parts.spake_a);
    hash.update(parts.spake_b);
    hash.finalize().into()
}

/// `(host confirm key, client confirm key)` from the SPAKE2 key `k`.
pub fn confirm_keys(k: &[u8], t: &[u8; 32]) -> ([u8; 32], [u8; 32]) {
    let hkdf = Hkdf::<Sha256>::new(Some(t), k);
    let mut h = [0u8; 32];
    let mut c = [0u8; 32];
    // 32 bytes is far below the HKDF-SHA256 limit (8160), so `expand` cannot fail.
    let _ = hkdf.expand(b"confirm h", &mut h);
    let _ = hkdf.expand(b"confirm c", &mut c);
    (h, c)
}

fn hmac(key: &[u8; 32]) -> Hmac<Sha256> {
    // HMAC takes keys of any length; a 32-byte key never fails.
    match <Hmac<Sha256> as KeyInit>::new_from_slice(key) {
        Ok(mac) => mac,
        Err(_) => unreachable!("HMAC accepts any key length"),
    }
}

pub fn mac(key: &[u8; 32], t: &[u8; 32]) -> [u8; 32] {
    let mut mac = hmac(key);
    mac.update(t);
    mac.finalize().into_bytes().into()
}

/// Constant-time check of a received MAC.
pub fn verify(key: &[u8; 32], t: &[u8; 32], received: &[u8]) -> bool {
    let mut mac = hmac(key);
    mac.update(t);
    mac.verify_slice(received).is_ok()
}

#[cfg(test)]
#[path = "transcript_tests.rs"]
mod tests;
