//! Pipe listener and per-connection threads. Control plane: blocking I/O, one thread per client.

use std::fs::File;
use std::io::{BufReader, Write};
use std::os::windows::io::OwnedHandle;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use mixlink_core::ipc::encode_line;
use mixlink_core::svc_ipc::{LineRead, MAX_LINE, read_line_limited};

use crate::net_check::{PUBLIC_CHECK_TTL, PublicCheck};
use crate::pipe::session::{PairingPort, Reply, Session, too_long_reply};
use crate::platform::windows::network_category::any_public_network;
use crate::platform::windows::pipe::{create_instance, wait_client};
use crate::platform::windows::security::PipeSecurity;
use crate::stop::StopSignal;

/// Pause before retrying while all pipe instances are taken.
const BUSY_RETRY: Duration = Duration::from_millis(100);

/// Serves clients until `stop` is set and the listener is woken (see `run`).
/// `first` is the already-created first instance.
pub fn listen(
    name: &str,
    first: OwnedHandle,
    security: &PipeSecurity,
    stop: &StopSignal,
    started: Instant,
    pairing: &Arc<dyn PairingPort + Send + Sync>,
) {
    let mut next = Some(first);
    while !stop.is_stopped() {
        let instance = match next.take() {
            Some(instance) => instance,
            None => match create_instance(name, false, security) {
                Ok(Some(instance)) => instance,
                Ok(None) => {
                    stop.wait_timeout(BUSY_RETRY);
                    continue;
                }
                Err(error) => {
                    log::error!("creating pipe instance: {error}");
                    stop.wait_timeout(BUSY_RETRY);
                    continue;
                }
            },
        };
        if let Err(error) = wait_client(&instance) {
            log::warn!("waiting for a pipe client: {error}");
            continue;
        }
        if stop.is_stopped() {
            break;
        }
        let pairing = Arc::clone(pairing);
        let spawned = thread::Builder::new()
            .name("svc-pipe-conn".into())
            .spawn(move || serve(File::from(instance), started, pairing.as_ref()));
        if let Err(error) = spawned {
            log::error!("spawning pipe connection thread: {error}");
        }
    }
}

/// A failed check reads as "not Public": the hint is advice, not worth an error.
fn probe_public() -> bool {
    any_public_network().unwrap_or_else(|error| {
        log::debug!("network category check failed: {error}");
        false
    })
}

/// Answers one client until it disconnects or sends something unexpected.
fn serve(file: File, started: Instant, pairing: &dyn PairingPort) {
    let mut reader = BufReader::new(&file);
    let mut writer = &file;
    let mut session = Session::new();
    let mut public = PublicCheck::new(PUBLIC_CHECK_TTL);
    let mut line = Vec::new();
    loop {
        let reply = match read_line_limited(&mut reader, &mut line, MAX_LINE) {
            Ok(LineRead::Line) => session.handle_line(
                &line,
                started.elapsed(),
                || public.get(Instant::now(), probe_public),
                pairing,
            ),
            Ok(LineRead::TooLong) => too_long_reply(),
            Ok(LineRead::Eof) => return,
            Err(error) => {
                log::debug!("pipe client read: {error}");
                return;
            }
        };
        if let Err(error) = send(&mut writer, &reply) {
            log::debug!("pipe client write: {error}");
            return;
        }
        if reply.close {
            // FlushFileBuffers: the client gets the reply before the handle closes.
            let _ = file.sync_all();
            return;
        }
    }
}

fn send(writer: &mut &File, reply: &Reply) -> std::io::Result<()> {
    let line = encode_line(&reply.msg).map_err(std::io::Error::other)?;
    writer.write_all(line.as_bytes())
}
