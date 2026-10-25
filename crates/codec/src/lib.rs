//! Hardware video encode (host) and decode (client): H.264/HEVC/AV1, low-latency settings, intra-refresh.

mod codec;
pub mod error;
mod ffi;
pub mod parser;

pub use codec::VideoCodec;
pub use error::CodecError;
pub use parser::AccessUnitParser;
