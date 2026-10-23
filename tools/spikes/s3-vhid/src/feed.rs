//! Backend-neutral feeding interface (vhid, SendInput, ViGEm).

use std::fmt;

use crate::input::Input;
use crate::report::ReportError;

#[derive(Debug)]
pub enum FeedError {
    Report(ReportError),
    /// The backend cannot deliver this kind of input (e.g. a pad through SendInput).
    Unsupported(&'static str),
    /// A system call failed; text includes the call and the error.
    Os(String),
}

impl fmt::Display for FeedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FeedError::Report(e) => write!(f, "report: {e}"),
            FeedError::Unsupported(what) => write!(f, "unsupported: {what}"),
            FeedError::Os(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for FeedError {}

impl From<ReportError> for FeedError {
    fn from(e: ReportError) -> Self {
        FeedError::Report(e)
    }
}

pub trait Feeder {
    /// Delivers one event. HOT PATH: no allocation in implementations.
    fn send(&mut self, input: &Input) -> Result<(), FeedError>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    Vhid,
    SendInput,
    Vigem,
}

impl Backend {
    pub fn parse(name: &str) -> Option<Backend> {
        [Backend::Vhid, Backend::SendInput, Backend::Vigem]
            .into_iter()
            .find(|b| format!("{b:?}").eq_ignore_ascii_case(name))
    }
}

/// Hex bytes, spaces allowed ("05 09 ff"). None on odd length or a bad digit.
pub fn parse_hex(text: &str) -> Option<Vec<u8>> {
    let digits: Vec<u8> = text.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    if digits.is_empty() || !digits.len().is_multiple_of(2) {
        return None;
    }
    digits
        .chunks(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok())
        .collect()
}

#[cfg(test)]
#[path = "feed_tests.rs"]
mod tests;
