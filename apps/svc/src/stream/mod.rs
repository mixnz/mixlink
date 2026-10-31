//! Per-session stream (spec 0020): the client's monitor, capture → encode → sealed video, the hardware
//! cursor, and feedback from the client. Started and stopped by `svc-session`.

pub mod cursor_send;
pub mod feedback;
pub mod gaps;
pub mod input_recv;
pub mod input_service;
#[cfg(windows)]
pub mod input_thread;
#[cfg(windows)]
pub mod net_sink;
#[cfg(windows)]
pub mod session;
pub mod streamer;
pub mod video_send;
