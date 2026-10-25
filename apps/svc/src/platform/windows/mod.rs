//! Windows-only code; every `unsafe` block of the service lives here.

pub mod catalog;
pub mod cert;
pub mod cert_der;
pub mod console;
pub mod install;
pub mod pipe;
pub mod security;
pub mod service;
pub mod sign;
pub mod trust_store;
