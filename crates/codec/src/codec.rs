//! Video codecs the client can decode.

/// 8-bit 4:2:0 video codecs (spec 0013).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VideoCodec {
    H264,
    Hevc,
}

impl VideoCodec {
    /// Codec of an Annex-B elementary stream file, from its extension (case-insensitive).
    pub fn from_extension(ext: &str) -> Option<VideoCodec> {
        match ext.to_ascii_lowercase().as_str() {
            "h264" | "264" => Some(VideoCodec::H264),
            "hevc" | "h265" | "265" => Some(VideoCodec::Hevc),
            _ => None,
        }
    }
}
