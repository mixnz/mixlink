//! Host and client config: defaults, validation and JSON text form. Where the file lives is
//! up to each app. Missing fields take their defaults; unknown fields are ignored.

use serde::Deserialize;
use serde::Serialize;

use crate::error::ConfigError;

/// Bumped on any breaking change to the config shape.
pub const CONFIG_VERSION: u32 = 1;

/// Device name limit in UTF-8 bytes (mDNS label); config and the control handshake share it.
pub const MAX_NAME_BYTES: usize = 63;
const FPS_RANGE: std::ops::RangeInclusive<u32> = 1..=240;
const BITRATE_KBPS_RANGE: std::ops::RangeInclusive<u32> = 1_000..=500_000;

/// Screen layout used while a client is streaming (ADR-0006). Default: the virtual monitor only.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Topology {
    /// The virtual monitor is the main and only display; physical monitors are off.
    #[default]
    VirtualOnly,
    /// The virtual monitor is the main display; physical monitors stay on to its right.
    Extend,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct NetConfig {
    pub discovery_port: u16,
    pub session_port: u16,
}

impl Default for NetConfig {
    fn default() -> Self {
        Self {
            discovery_port: 47800,
            session_port: 47801,
        }
    }
}

impl NetConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.discovery_port == 0 {
            return Err(ConfigError::ZeroPort {
                field: "discovery_port",
            });
        }
        if self.session_port == 0 {
            return Err(ConfigError::ZeroPort {
                field: "session_port",
            });
        }
        if self.discovery_port == self.session_port {
            return Err(ConfigError::PortClash {
                port: self.session_port,
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct VideoConfig {
    pub max_fps: u32,
    pub bitrate_kbps: u32,
}

impl Default for VideoConfig {
    fn default() -> Self {
        Self {
            max_fps: 120,
            bitrate_kbps: 50_000,
        }
    }
}

impl VideoConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if !FPS_RANGE.contains(&self.max_fps) {
            return Err(ConfigError::FpsOutOfRange {
                value: self.max_fps,
            });
        }
        if !BITRATE_KBPS_RANGE.contains(&self.bitrate_kbps) {
            return Err(ConfigError::BitrateOutOfRange {
                value: self.bitrate_kbps,
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct HostConfig {
    pub version: u32,
    pub name: String,
    pub net: NetConfig,
    pub video: VideoConfig,
}

impl Default for HostConfig {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            name: "MixLink Host".to_owned(),
            net: NetConfig::default(),
            video: VideoConfig::default(),
        }
    }
}

impl HostConfig {
    /// Parses and validates.
    pub fn from_json(text: &str) -> Result<Self, ConfigError> {
        let config: Self = serde_json::from_str(text).map_err(ConfigError::Parse)?;
        config.validate()?;
        Ok(config)
    }

    pub fn to_json_pretty(&self) -> Result<String, ConfigError> {
        serde_json::to_string_pretty(self).map_err(ConfigError::Encode)
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        validate_version(self.version)?;
        validate_name(&self.name)?;
        self.net.validate()?;
        self.video.validate()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ClientConfig {
    pub version: u32,
    pub name: String,
    pub net: NetConfig,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            name: "MixLink Client".to_owned(),
            net: NetConfig::default(),
        }
    }
}

impl ClientConfig {
    /// Parses and validates.
    pub fn from_json(text: &str) -> Result<Self, ConfigError> {
        let config: Self = serde_json::from_str(text).map_err(ConfigError::Parse)?;
        config.validate()?;
        Ok(config)
    }

    pub fn to_json_pretty(&self) -> Result<String, ConfigError> {
        serde_json::to_string_pretty(self).map_err(ConfigError::Encode)
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        validate_version(self.version)?;
        validate_name(&self.name)?;
        self.net.validate()
    }
}

fn validate_version(found: u32) -> Result<(), ConfigError> {
    if found == CONFIG_VERSION {
        Ok(())
    } else {
        Err(ConfigError::UnsupportedVersion {
            found,
            expected: CONFIG_VERSION,
        })
    }
}

/// The 63-byte cap counts UTF-8 bytes (mDNS label limit), so accented names hold fewer chars.
fn validate_name(name: &str) -> Result<(), ConfigError> {
    if name.trim().is_empty() {
        return Err(ConfigError::EmptyName);
    }
    if name.len() > MAX_NAME_BYTES {
        return Err(ConfigError::NameTooLong { len: name.len() });
    }
    Ok(())
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
