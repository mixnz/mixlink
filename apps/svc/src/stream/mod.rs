//! Per-session stream (spec 0020): the client's monitor, capture → encode → sealed video, the hardware
//! cursor, and feedback from the client. Started and stopped by `svc-session`.

pub mod cursor_send;
pub mod feedback;
pub mod gaps;
#[cfg(windows)]
pub mod net_sink;
#[cfg(windows)]
pub mod session;
pub mod streamer;
pub mod video_send;
