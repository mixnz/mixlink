//! Virtual display control (spec 0015): driver wire format, EDID, modes.

pub mod agent;
pub mod cvt;
pub mod edid;
pub mod modes;
pub mod status;
pub mod store;
pub mod topology;

#[cfg(windows)]
pub mod commands;
pub mod ports;
#[cfg(windows)]
pub mod service;
pub mod session;
pub mod wire;
