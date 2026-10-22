//! Spike S3 (spec 0006): virtual HID input harness. Throwaway code.

pub mod feed;
pub mod ident;
pub mod input;
pub mod log;
#[cfg(windows)]
pub mod platform;
pub mod report;
pub mod scenario;
pub mod seq;
pub mod stats;
