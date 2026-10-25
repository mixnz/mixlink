//! Play mode source: reads an Annex-B file and sends its access units at `--fps`, standing in
//! for the network receive path (spec 0013).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use std::{fs, io};

use mixlink_codec::AccessUnitParser;

use crate::args::PlayArgs;
use crate::au_queue::AuSender;

/// The running feeder thread; `stop` (or drop) ends it.
#[derive(Debug)]
pub struct Feeder {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl Feeder {
    /// Reads the whole file now (errors come back here), then sends from a `feeder` thread.
    pub fn start(args: &PlayArgs, tx: AuSender) -> io::Result<Feeder> {
        let data = fs::read(&args.path)?;
        let parser = AccessUnitParser::new(args.codec).map_err(io::Error::other)?;
        let period = Duration::from_secs(1) / args.fps.max(1);
        let looping = args.looping;
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        let handle = thread::Builder::new()
            .name("feeder".into())
            .spawn(move || {
                let mut pacer = Pacer::new(period, flag);
                run(&data, parser, tx, looping, &mut pacer);
            })?;
        Ok(Feeder {
            stop,
            handle: Some(handle),
        })
    }

    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for Feeder {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Sleeps to fixed deadlines one period apart; never sends a burst to catch up.
struct Pacer {
    period: Duration,
    next: Option<Instant>,
    stop: Arc<AtomicBool>,
}

impl Pacer {
    fn new(period: Duration, stop: Arc<AtomicBool>) -> Self {
        Self {
            period,
            next: None,
            stop,
        }
    }

    /// Waits for the next deadline in short naps; `false` once stop is asked.
    fn wait(&mut self) -> bool {
        let now = Instant::now();
        let due = match self.next {
            Some(next) if next > now => next,
            // First AU, or late: send now and restart the schedule from here.
            _ => now,
        };
        while !self.stop.load(Ordering::Relaxed) {
            let left = due.saturating_duration_since(Instant::now());
            if left.is_zero() {
                self.next = Some(due + self.period);
                return true;
            }
            thread::sleep(left.min(Duration::from_millis(20)));
        }
        false
    }
}

fn run(
    data: &[u8],
    mut parser: AccessUnitParser,
    mut tx: AuSender,
    looping: bool,
    pacer: &mut Pacer,
) {
    loop {
        let mut input = data;
        while !input.is_empty() {
            match parser.push(&mut input) {
                Ok(Some(au)) => {
                    if !pacer.wait() {
                        return;
                    }
                    // A full queue drops the AU, like network loss (counted by the queue).
                    let _ = tx.send(au);
                }
                Ok(None) => {}
                Err(e) => {
                    log::error!("play file: {e}");
                    return;
                }
            }
        }
        while let Ok(Some(au)) = parser.flush() {
            if !pacer.wait() {
                return;
            }
            let _ = tx.send(au);
        }
        let counts = tx.counts();
        log::info!(
            "play file sent; dropped full {}, oversize {}",
            counts.full,
            counts.oversize
        );
        if !looping {
            return;
        }
        if let Err(e) = parser.reset() {
            log::error!("play file: {e}");
            return;
        }
    }
}

#[cfg(test)]
#[path = "feeder_tests.rs"]
mod tests;
