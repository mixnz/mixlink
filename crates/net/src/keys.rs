//! Device keys and randomness for pairing and sessions (spec 0018). Control plane.

use std::fmt;

use mixlink_core::hex;
use mixlink_core::ids::SessionId;
use mixlink_proto::control::PublicKey;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::error::NetError;
use crate::session::noise::NOISE_PATTERN;

/// A device's long-term X25519 key pair. `Debug` never shows the private key.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StaticKeyPair {
    #[serde(serialize_with = "private_hex", deserialize_with = "private_from_hex")]
    pub private: [u8; 32],
    pub public: PublicKey,
}

impl fmt::Debug for StaticKeyPair {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StaticKeyPair")
            .field("public", &self.public)
            .finish_non_exhaustive()
    }
}

impl StaticKeyPair {
    /// A fresh key pair from the OS RNG.
    pub fn generate() -> Result<StaticKeyPair, NetError> {
        let builder = snow::Builder::new(
            NOISE_PATTERN
                .parse()
                .map_err(|error| NetError::Keys(format!("{error}")))?,
        );
        let pair = builder
            .generate_keypair()
            .map_err(|error| NetError::Keys(format!("{error}")))?;
        let private = to_key(&pair.private)?;
        let public = PublicKey(to_key(&pair.public)?);
        Ok(StaticKeyPair { private, public })
    }
}

fn to_key(bytes: &[u8]) -> Result<[u8; 32], NetError> {
    bytes
        .try_into()
        .map_err(|_| NetError::Keys(format!("key is {} bytes, expected 32", bytes.len())))
}

fn private_hex<S: Serializer>(key: &[u8; 32], serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&hex::encode(key))
}

fn private_from_hex<'de, D: Deserializer<'de>>(deserializer: D) -> Result<[u8; 32], D::Error> {
    let text = String::deserialize(deserializer)?;
    let mut key = [0u8; 32];
    hex::decode_into(&text, &mut key).map_err(serde::de::Error::custom)?;
    Ok(key)
}

/// Largest multiple of 1 000 000 that fits a `u32`: draws at or above it are redrawn, so every
/// PIN is equally likely.
const PIN_LIMIT: u32 = 4_294_000_000;

/// A uniform 6-digit PIN, `0..=999_999`.
pub fn random_pin() -> Result<u32, NetError> {
    loop {
        let draw = getrandom::u32().map_err(|error| NetError::Keys(format!("{error}")))?;
        if draw < PIN_LIMIT {
            return Ok(draw % 1_000_000);
        }
    }
}

/// A random session id.
pub fn random_session() -> Result<SessionId, NetError> {
    getrandom::u32()
        .map(SessionId::new)
        .map_err(|error| NetError::Keys(format!("{error}")))
}

#[cfg(test)]
#[path = "keys_tests.rs"]
mod tests;
