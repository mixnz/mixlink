//! Windows-only code; every `unsafe` block of the service lives here.

pub mod console;
pub mod pipe;
pub mod security;
