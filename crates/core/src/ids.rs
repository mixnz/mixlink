//! Id newtypes shared by every crate.

use std::fmt;
use std::str::FromStr;

use serde::Deserialize;
use serde::Deserializer;
use serde::Serialize;
use serde::Serializer;

use crate::error::{HexError, IdParseError};
use crate::hex;

/// One streaming session. The value is chosen by the caller (random once pairing lands).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SessionId(u32);

impl SessionId {
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Video/audio frame number; wraps at `u32::MAX`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FrameId(u32);

impl FrameId {
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u32 {
        self.0
    }

    pub const fn next(self) -> Self {
        Self(self.0.wrapping_add(1))
    }

    /// Serial-number order (RFC 1982): true if `self` comes after `other`.
    pub const fn is_newer_than(self, other: Self) -> bool {
        serial_newer(self.0, other.0)
    }
}

/// Per-channel packet sequence; wraps at `u32::MAX`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Seq(u32);

impl Seq {
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u32 {
        self.0
    }

    pub const fn next(self) -> Self {
        Self(self.0.wrapping_add(1))
    }

    /// Serial-number order (RFC 1982): true if `self` comes after `other`.
    pub const fn is_newer_than(self, other: Self) -> bool {
        serial_newer(self.0, other.0)
    }
}

/// Values exactly 2^31 apart are newer in neither direction.
const fn serial_newer(a: u32, b: u32) -> bool {
    (a.wrapping_sub(b) as i32) > 0
}

/// Stable machine id; text form is 32 lowercase hex digits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DeviceId([u8; 16]);

impl DeviceId {
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

impl fmt::Display for DeviceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for b in self.0 {
            write!(f, "{b:02x}")?;
        }
        Ok(())
    }
}

impl FromStr for DeviceId {
    type Err = IdParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let text = s.as_bytes();
        if text.len() != 32 {
            return Err(IdParseError::Length { len: text.len() });
        }
        let mut bytes = [0u8; 16];
        hex::decode_into(s, &mut bytes).map_err(|error| match error {
            HexError::NotHex { index } => IdParseError::NotHex { index },
            HexError::Length { len, .. }
            | HexError::OddLength { len }
            | HexError::TooLong { len, .. } => IdParseError::Length { len },
        })?;
        Ok(Self(bytes))
    }
}

impl Serialize for DeviceId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for DeviceId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
#[path = "ids_tests.rs"]
mod tests;
