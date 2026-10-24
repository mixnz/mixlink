//! The service's work, shared by service mode and `--console`.

use std::fs::OpenOptions;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use crate::error::SvcError;
use crate::pipe::server::listen;
use crate::platform::windows::pipe::create_instance;
use crate::platform::windows::security::PipeSecurity;
use crate::stop::StopSignal;

/// How long `run` keeps poking the listener awake after a stop.
const WAKE_TIMEOUT: Duration = Duration::from_secs(2);
const WAKE_RETRY: Duration = Duration::from_millis(20);

#[derive(Debug, Clone)]
pub struct RunOptions {
    pub pipe: String,
}

/// Serves the pipe until `stop` is set. `on_ready` runs once the pipe accepts clients;
/// errors before that (pipe taken, bad ACL) are returned without calling it.
pub fn run(
    stop: &Arc<StopSignal>,
    opts: &RunOptions,
    on_ready: impl FnOnce(),
) -> Result<(), SvcError> {
    let started = Instant::now();
    let security = Arc::new(PipeSecurity::new()?);
    let first = create_instance(&opts.pipe, true, &security)?.ok_or(SvcError::PipeTaken)?;
    let listener = {
        let name = opts.pipe.clone();
        let stop = Arc::clone(stop);
        let security = Arc::clone(&security);
        thread::Builder::new()
            .name("svc-pipe-listen".into())
            .spawn(move || listen(&name, first, &security, &stop, started))?
    };
    log::info!("serving {}", opts.pipe);
    on_ready();
    stop.wait();
    wake_listener(&opts.pipe, &listener);
    if listener.join().is_err() {
        log::error!("pipe listener panicked");
    }
    log::info!("stopped");
    Ok(())
}

/// The listener blocks in `ConnectNamedPipe`; a throwaway client connection lets it see the stop.
/// Retried because the listener may be between instances when the first attempt runs.
fn wake_listener(name: &str, listener: &thread::JoinHandle<()>) {
    let deadline = Instant::now() + WAKE_TIMEOUT;
    while !listener.is_finished() && Instant::now() < deadline {
        let _ = OpenOptions::new().read(true).write(true).open(name);
        thread::sleep(WAKE_RETRY);
    }
}

#[cfg(test)]
#[path = "run_tests.rs"]
mod tests;
