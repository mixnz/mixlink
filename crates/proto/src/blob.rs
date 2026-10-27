//! Byte fields of control messages (spec 0018), carried as lowercase hex in JSON.

use std::fmt;

use mixlink_core::hex;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Longest `Blob` accepted when parsing; message rules check tighter limits.
pub const MAX_BLOB: usize = 4096;

/// An X25519 public key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PublicKey(pub [u8; 32]);

impl fmt::Display for PublicKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&hex::encode(&self.0))
    }
}

impl Serialize for PublicKey {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&hex::encode(&self.0))
    }
}

impl<'de> Deserialize<'de> for PublicKey {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        let mut key = [0u8; 32];
        hex::decode_into(&text, &mut key).map_err(serde::de::Error::custom)?;
        Ok(PublicKey(key))
    }
}

/// Opaque bytes: a SPAKE2 or Noise message, or a MAC.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Blob(pub Vec<u8>);

impl Serialize for Blob {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&hex::encode(&self.0))
    }
}

impl<'de> Deserialize<'de> for Blob {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        hex::decode_vec(&text, MAX_BLOB)
            .map(Blob)
            .map_err(serde::de::Error::custom)
    }
}
