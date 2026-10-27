//! What a host advertises (spec 0017): the same fields in mDNS TXT records and broadcast replies.

use mixlink_core::config::MAX_NAME_BYTES;
use mixlink_core::ids::DeviceId;
use serde::{Deserialize, Serialize};

pub const SERVICE_TYPE: &str = "_mixlink._udp.local";
/// The only capability so far; receivers ignore values they do not know.
pub const CAPS_V1: &str = "v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Advert {
    pub id: DeviceId,
    pub name: String,
    /// Wire protocol version (`proto::header::WIRE_VERSION`).
    pub ver: u8,
    /// Comma-separated capability list.
    pub caps: String,
    /// Session port (mDNS: SRV record).
    pub port: u16,
}

impl Advert {
    /// TXT key / value pairs; the port travels in the SRV record.
    pub fn txt(&self) -> [(&'static str, String); 4] {
        [
            ("id", self.id.to_string()),
            ("name", self.name.clone()),
            ("ver", self.ver.to_string()),
            ("caps", self.caps.clone()),
        ]
    }

    /// Parses TXT pairs (keys compared ignoring ASCII case, as DNS-SD does). Missing or bad
    /// `id` / `name` / `ver` → `None`; missing `caps` → empty.
    pub fn from_txt(pairs: &[(String, String)], port: u16) -> Option<Advert> {
        let get = |key: &str| {
            pairs
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(key))
                .map(|(_, v)| v.as_str())
        };
        let advert = Advert {
            id: get("id")?.parse().ok()?,
            name: get("name")?.to_owned(),
            ver: get("ver")?.parse().ok()?,
            caps: get("caps").unwrap_or_default().to_owned(),
            port,
        };
        advert.is_valid().then_some(advert)
    }

    pub fn caps_list(&self) -> impl Iterator<Item = &str> {
        self.caps.split(',').filter(|cap| !cap.is_empty())
    }

    /// Same name rule as the config files: not blank, at most `MAX_NAME_BYTES`.
    pub fn is_valid(&self) -> bool {
        !self.name.trim().is_empty() && self.name.len() <= MAX_NAME_BYTES
    }
}

#[cfg(test)]
#[path = "advert_tests.rs"]
mod tests;
