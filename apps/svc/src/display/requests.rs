//! Monitor requests to the display thread (spec 0020): only `svc-display` holds the driver handle and the
//! headless monitor, so a stream asks it to begin / end a client session and waits for the answer.
//! Control plane.

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::Duration;

use mixlink_core::config::Topology;
use mixlink_core::ids::DeviceId;
use mixlink_proto::control::DisplayInfo;

use super::ports::{DisplayConfig, DisplayDriver};
use super::session::{self, Active, Ports};
use crate::error::SvcError;
use crate::stop::StopSignal;

/// How often `serve` looks at the stop signal.
const POLL: Duration = Duration::from_millis(100);

/// A client monitor that is up and the connector its frames are shared on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamTarget {
    pub active: Active,
    pub connector: u32,
}

#[derive(Debug)]
pub enum DisplayRequest {
    Begin {
        client: DeviceId,
        mode: DisplayInfo,
        reply: Sender<Result<StreamTarget, SvcError>>,
    },
    End {
        active: Active,
        reply: Sender<Result<(), SvcError>>,
    },
}

/// Sends requests to the display thread; blocking calls.
#[derive(Debug, Clone)]
pub struct DisplayHandle(Sender<DisplayRequest>);

impl DisplayHandle {
    pub fn channel() -> (DisplayHandle, Receiver<DisplayRequest>) {
        let (tx, rx) = mpsc::channel();
        (DisplayHandle(tx), rx)
    }

    /// Adds `client`'s monitor in `mode` and switches the layout. `DisplayNotReady` when the display
    /// thread is gone (driver missing at start).
    pub fn begin(&self, client: DeviceId, mode: DisplayInfo) -> Result<StreamTarget, SvcError> {
        let (reply, answer) = mpsc::channel();
        self.0
            .send(DisplayRequest::Begin {
                client,
                mode,
                reply,
            })
            .map_err(|_| SvcError::DisplayNotReady)?;
        answer.recv().map_err(|_| SvcError::DisplayNotReady)?
    }

    /// Removes the monitor and restores the layout.
    pub fn end(&self, active: Active) -> Result<(), SvcError> {
        let (reply, answer) = mpsc::channel();
        self.0
            .send(DisplayRequest::End { active, reply })
            .map_err(|_| SvcError::DisplayNotReady)?;
        answer.recv().map_err(|_| SvcError::DisplayNotReady)?
    }
}

/// Answers requests until `stop`.
pub fn serve<D: DisplayDriver, C: DisplayConfig>(
    ports: &mut Ports<'_, D, C>,
    headless: &mut Option<u32>,
    topology: Option<Topology>,
    inbox: &Receiver<DisplayRequest>,
    stop: &StopSignal,
    sleep: &dyn Fn(Duration),
) {
    while !stop.is_stopped() {
        let request = match inbox.recv_timeout(POLL) {
            Ok(request) => request,
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => {
                stop.wait();
                return;
            }
        };
        match request {
            DisplayRequest::Begin {
                client,
                mode,
                reply,
            } => {
                let target = begin(ports, headless, topology, &client, &mode, sleep);
                if let Err(error) = &target {
                    log::warn!("client monitor not added: {error}");
                }
                // The stream gave up waiting only when it is gone; nothing to answer then.
                let _ = reply.send(target);
            }
            DisplayRequest::End { active, reply } => {
                let ended = session::end(ports, active, headless);
                if let Err(error) = &ended {
                    log::warn!("client monitor not removed cleanly: {error}");
                }
                let _ = reply.send(ended);
            }
        }
    }
}

fn begin<D: DisplayDriver, C: DisplayConfig>(
    ports: &mut Ports<'_, D, C>,
    headless: &mut Option<u32>,
    topology: Option<Topology>,
    client: &DeviceId,
    mode: &DisplayInfo,
    sleep: &dyn Fn(Duration),
) -> Result<StreamTarget, SvcError> {
    let active = session::begin(ports, client, mode, topology, headless, sleep)?;
    match connector_of(ports.driver, active.monitor) {
        Ok(connector) => Ok(StreamTarget { active, connector }),
        Err(error) => {
            if let Err(undo) = session::end(ports, active, headless) {
                log::warn!("client monitor not removed after an error: {undo}");
            }
            Err(error)
        }
    }
}

/// The frame-share connector of a monitor the driver reports.
pub fn connector_of<D: DisplayDriver>(driver: &mut D, monitor: u32) -> Result<u32, SvcError> {
    driver
        .query()?
        .monitors
        .iter()
        .find(|m| m.id == monitor)
        .map(|m| m.connector)
        .ok_or(SvcError::DisplayNotReady)
}

#[cfg(test)]
#[path = "requests_tests.rs"]
mod tests;
