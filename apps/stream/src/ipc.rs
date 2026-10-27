//! Stream side of the launcher IPC: commands in on stdin, status out on stdout.
//! Stdout is reserved for these messages; logs go to stderr.

use std::io::{self, BufRead, Write};

use mixlink_core::ipc::{
    CloseReason, LauncherToStream, StreamToLauncher, decode_line, encode_line,
};
use mixlink_proto::control::{CodecId, DisplayInfo};
use mixlink_stream::video::VideoEvent;

/// Event forwarded to the window event loop (stdin thread, video thread).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamEvent {
    Stop,
    /// Stdin closed or failed: the launcher is gone.
    LauncherGone,
    /// From the video thread.
    Video(VideoEvent),
    /// From `stream-net` (spec 0020).
    Net(NetEvent),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetEvent {
    Connected {
        mode: DisplayInfo,
        codec: CodecId,
    },
    /// The session ended from the host side or the network.
    Ended(CloseReason),
}

/// Reads launcher commands until EOF. Undecodable lines are logged and skipped.
pub fn read_commands<R: BufRead>(reader: R, mut on_event: impl FnMut(StreamEvent)) {
    for line in reader.lines() {
        let line = match line {
            Ok(line) => line,
            Err(error) => {
                log::warn!("stdin read failed: {error}");
                break;
            }
        };
        match decode_line::<LauncherToStream>(&line) {
            Ok(LauncherToStream::Stop) => on_event(StreamEvent::Stop),
            Err(error) => log::warn!("ignoring launcher line: {error}"),
        }
    }
    on_event(StreamEvent::LauncherGone);
}

/// Writes one message line and flushes so the launcher sees it at once.
pub fn send<W: Write>(out: &mut W, msg: &StreamToLauncher) -> io::Result<()> {
    let line = encode_line(msg).map_err(io::Error::other)?;
    out.write_all(line.as_bytes())?;
    out.flush()
}

#[cfg(test)]
#[path = "ipc_tests.rs"]
mod tests;
