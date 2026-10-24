//! Host tray <-> `mixlink-svc` messages over a local named pipe (ADR-0005, spec 0012).
//! JSON lines, same codec as `ipc`. Control plane only.

use std::fs::{File, OpenOptions};
use std::io::{self, BufRead, BufReader, Write};
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::ipc::{IpcError, decode_line, encode_line};

/// Bumped on any breaking change to the messages below.
pub const PIPE_VERSION: u32 = 1;

/// The service's pipe; tests and `--console --pipe` use other names.
pub const PIPE_NAME: &str = r"\\.\pipe\mixlink-svc";

/// Longest accepted line in bytes, `\n` included. The service drops a client that sends more.
pub const MAX_LINE: usize = 64 * 1024;

/// Windows `ERROR_PIPE_BUSY`: no free instance right now (all taken, or the server is between two).
const ERROR_PIPE_BUSY: i32 = 231;

/// Windows access rights for the client end (winnt.h).
#[cfg(windows)]
const FILE_GENERIC_READ: u32 = 0x0012_0089;
#[cfg(windows)]
const FILE_WRITE_DATA: u32 = 0x0002;

/// How long `SvcPipe::connect` retries a busy pipe before reporting `NotRunning`.
const BUSY_WAIT: Duration = Duration::from_millis(250);
const BUSY_RETRY: Duration = Duration::from_millis(10);

/// Sent by the tray. `Hello` must come first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum TrayToSvc {
    Hello { pipe_version: u32 },
    GetStatus,
}

/// Sent by the service, one reply per request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SvcToTray {
    /// `svc_version` is the service's crate version.
    Hello {
        pipe_version: u32,
        svc_version: String,
    },
    Status {
        state: SvcState,
        uptime_s: u64,
    },
    /// The request was refused; the service closes the connection after it.
    Error {
        message: String,
    },
}

/// What the service is doing; streaming and pairing states come with those features.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SvcState {
    Idle,
}

/// Result of [`read_line_limited`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineRead {
    /// `buf` holds one line, with its `\n` unless the stream ended first.
    Line,
    /// More than `max` bytes came before a `\n`; `buf` content is undefined.
    TooLong,
    /// The stream ended with no bytes.
    Eof,
}

/// Reads one line into `buf` (cleared first) without ever holding more than `max` bytes.
pub fn read_line_limited<R: BufRead>(
    reader: &mut R,
    buf: &mut Vec<u8>,
    max: usize,
) -> io::Result<LineRead> {
    buf.clear();
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            return Ok(if buf.is_empty() {
                LineRead::Eof
            } else {
                LineRead::Line
            });
        }
        let (take, done) = match available.iter().position(|&b| b == b'\n') {
            Some(at) => (at + 1, true),
            None => (available.len(), false),
        };
        if buf.len() + take > max {
            return Ok(LineRead::TooLong);
        }
        buf.extend_from_slice(&available[..take]);
        reader.consume(take);
        if done {
            return Ok(LineRead::Line);
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SvcPipeError {
    #[error("the MixLink service is not running")]
    NotRunning,
    #[error("service pipe I/O failed: {0}")]
    Io(#[source] io::Error),
    #[error(transparent)]
    Ipc(#[from] IpcError),
    #[error("service pipe version {theirs} (service {svc_version}), tray expects {ours}")]
    VersionMismatch {
        ours: u32,
        theirs: u32,
        svc_version: String,
    },
    #[error("unexpected reply from the service")]
    Unexpected,
}

/// Blocking client end of the service pipe (used by the tray and tests).
#[derive(Debug)]
pub struct SvcPipe {
    reader: BufReader<File>,
    writer: File,
    line: Vec<u8>,
}

impl SvcPipe {
    /// Opens the pipe and exchanges `Hello`; returns the service's version.
    pub fn connect(name: &str) -> Result<(SvcPipe, String), SvcPipeError> {
        let file = open_pipe(name)?;
        let writer = file.try_clone().map_err(SvcPipeError::Io)?;
        let mut pipe = SvcPipe {
            reader: BufReader::new(file),
            writer,
            line: Vec::new(),
        };
        let reply = pipe.request(&TrayToSvc::Hello {
            pipe_version: PIPE_VERSION,
        })?;
        match reply {
            SvcToTray::Hello {
                pipe_version,
                svc_version,
            } if pipe_version == PIPE_VERSION => Ok((pipe, svc_version)),
            SvcToTray::Hello {
                pipe_version,
                svc_version,
            } => Err(SvcPipeError::VersionMismatch {
                ours: PIPE_VERSION,
                theirs: pipe_version,
                svc_version,
            }),
            SvcToTray::Status { .. } | SvcToTray::Error { .. } => Err(SvcPipeError::Unexpected),
        }
    }

    /// Sends one message and waits for its reply.
    pub fn request(&mut self, msg: &TrayToSvc) -> Result<SvcToTray, SvcPipeError> {
        let line = encode_line(msg)?;
        self.writer
            .write_all(line.as_bytes())
            .map_err(SvcPipeError::Io)?;
        match read_line_limited(&mut self.reader, &mut self.line, MAX_LINE)
            .map_err(SvcPipeError::Io)?
        {
            LineRead::Line => {}
            LineRead::Eof => {
                return Err(SvcPipeError::Io(io::ErrorKind::UnexpectedEof.into()));
            }
            LineRead::TooLong => return Err(SvcPipeError::Unexpected),
        }
        let text = std::str::from_utf8(&self.line).map_err(|_| SvcPipeError::Unexpected)?;
        Ok(decode_line(text)?)
    }

    /// Asks for the service state and uptime in seconds.
    pub fn status(&mut self) -> Result<(SvcState, u64), SvcPipeError> {
        match self.request(&TrayToSvc::GetStatus)? {
            SvcToTray::Status { state, uptime_s } => Ok((state, uptime_s)),
            SvcToTray::Hello { .. } | SvcToTray::Error { .. } => Err(SvcPipeError::Unexpected),
        }
    }
}

/// Read + write data only. `write(true)` would ask for `GENERIC_WRITE`, which includes
/// `FILE_CREATE_PIPE_INSTANCE`; the pipe ACL denies that to interactive users (spec 0012).
#[cfg(windows)]
fn client_options() -> OpenOptions {
    use std::os::windows::fs::OpenOptionsExt;
    let mut options = OpenOptions::new();
    options.access_mode(FILE_GENERIC_READ | FILE_WRITE_DATA);
    options
}

#[cfg(not(windows))]
fn client_options() -> OpenOptions {
    let mut options = OpenOptions::new();
    options.read(true).write(true);
    options
}

/// Opens the client end, retrying briefly while it is busy. `SvcPipe::connect` uses it.
pub fn open_pipe(name: &str) -> Result<File, SvcPipeError> {
    let deadline = Instant::now() + BUSY_WAIT;
    loop {
        match client_options().open(name) {
            Ok(file) => return Ok(file),
            Err(error) if error.raw_os_error() == Some(ERROR_PIPE_BUSY) => {
                if Instant::now() >= deadline {
                    return Err(SvcPipeError::NotRunning);
                }
                thread::sleep(BUSY_RETRY);
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Err(SvcPipeError::NotRunning);
            }
            Err(error) => return Err(SvcPipeError::Io(error)),
        }
    }
}

#[cfg(test)]
#[path = "svc_ipc_tests.rs"]
mod tests;
