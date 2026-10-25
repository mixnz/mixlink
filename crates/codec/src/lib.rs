//! Hardware video encode (host) and decode (client): H.264/HEVC/AV1, low-latency settings, intra-refresh.

mod codec;
#[cfg(windows)]
pub mod decoder;
pub mod error;
mod ffi;
pub mod parser;
mod platform;

pub use codec::VideoCodec;
#[cfg(windows)]
pub use decoder::{DecodedFrame, Decoder, EXTRA_SURFACES, has_hardware_decoder};
pub use error::CodecError;
pub use parser::AccessUnitParser;
