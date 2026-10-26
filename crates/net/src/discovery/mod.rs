//! LAN discovery (spec 0017): mDNS through the Windows DNS-SD API plus a UDP broadcast fallback.
//! Control plane only: plain threads, no hot-path rules.

pub mod advert;
pub mod advertiser;
pub mod browser;
pub mod error;
pub mod limiter;
pub mod list;
pub mod local_net;
pub mod wire;

pub use advert::{Advert, CAPS_V1, SERVICE_TYPE};
pub use advertiser::{Advertiser, AdvertiserConfig};
pub use browser::{Browser, BrowserConfig, DiscoveryEvent, OnChange, ProbeTargets};
pub use error::DiscoveryError;
pub use list::{EXPIRY, HostEntry, HostList, Source};
pub use local_net::LocalNet;
