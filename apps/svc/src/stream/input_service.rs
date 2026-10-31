//! Thread `svc-input` (spec 0021): lives as long as the session port and owns its Input lane, so the
//! lane always wakes the same thread (a lane wakes only the first thread that waited on it). Each stream
//! hands it a session (`InputControl::start`) and takes it back (`stop`: keys released, numbers logged);
//! between sessions datagrams are dropped.

use std::fmt::Display;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, Thread};
use std::time::Instant;

use mixlink_input::hid::report::ReportSink;
use mixlink_net::RxLane;

use super::input_recv::{InputRecv, WAIT};

#[derive(Debug)]
pub enum InputCommand {
    Start(Box<InputRecv>),
    Stop,
}

/// The streams' end: commands to `svc-input`, which is woken at once.
#[derive(Debug, Clone)]
pub struct InputControl {
    commands: Sender<InputCommand>,
    thread: Thread,
}

impl InputControl {
    pub fn new(commands: Sender<InputCommand>, thread: Thread) -> Self {
        Self { commands, thread }
    }

    pub fn start(&self, session: InputRecv) {
        self.send(InputCommand::Start(Box::new(session)));
    }

    pub fn stop(&self) {
        self.send(InputCommand::Stop);
    }

    fn send(&self, command: InputCommand) {
        if self.commands.send(command).is_err() {
            log::warn!("input thread gone; client keyboard and mouse are not applied");
        }
        self.thread.unpark();
    }
}

/// The command channel; the receiving end goes to `run`.
pub fn channel() -> (Sender<InputCommand>, Receiver<InputCommand>) {
    mpsc::channel()
}

/// The body of `svc-input` until `stopped()`; the session in progress is finished at the end.
pub fn run<S: ReportSink>(
    rx: &mut RxLane,
    sink: &mut S,
    commands: &Receiver<InputCommand>,
    stopped: &dyn Fn() -> bool,
) where
    S::Error: Display,
{
    rx.attach_current_thread();
    let mut session: Option<Box<InputRecv>> = None;
    while !stopped() {
        while let Ok(command) = commands.try_recv() {
            if let Some(mut old) = session.take() {
                old.finish(sink);
            }
            if let InputCommand::Start(next) = command {
                session = Some(next);
            }
        }
        match session.as_mut() {
            Some(recv) => recv.poll(rx, sink, WAIT, Instant::now()),
            // No session: drop what comes (a late packet of the last client).
            None => {
                while rx.try_recv().is_some() {}
                thread::park_timeout(WAIT);
            }
        }
    }
    if let Some(mut last) = session {
        last.finish(sink);
    }
}

#[cfg(test)]
#[path = "input_service_tests.rs"]
mod tests;
