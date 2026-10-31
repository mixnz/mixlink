//! Starts `svc-input` (spec 0021) for the session port: one thread for the service's life, feeding
//! the virtual input driver, or dropping input when the driver is missing.

use std::io;
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use mixlink_core::mmcss::{Mmcss, MmcssTask};
use mixlink_input::hid::report::{Report, ReportSink};
use mixlink_input::platform::windows::vhid::{VHid, VHidError};
use mixlink_net::RxLane;

use super::input_service::{InputControl, channel, run};
use crate::stop::StopSignal;

/// Where client input goes.
enum InputSink {
    Driver(Arc<VHid>),
    /// No `vhid`: input is dropped (logged once at start).
    Missing,
}

impl ReportSink for InputSink {
    type Error = VHidError;

    // HOT PATH
    fn feed(&mut self, report: &Report) -> Result<(), VHidError> {
        match self {
            InputSink::Driver(vhid) => vhid.feed(report),
            InputSink::Missing => Ok(()),
        }
    }
}

/// `svc-input` on the Input lane `rx`; it ends once `stop` is set.
pub fn spawn(
    mut rx: RxLane,
    vhid: Option<Arc<VHid>>,
    stop: Arc<StopSignal>,
) -> io::Result<(InputControl, JoinHandle<()>)> {
    let (commands, inbox) = channel();
    let handle = thread::Builder::new()
        .name("svc-input".into())
        .spawn(move || {
            let _mmcss = Mmcss::enter(MmcssTask::Games);
            let mut sink = match vhid {
                Some(vhid) => InputSink::Driver(vhid),
                None => {
                    log::warn!("virtual input unavailable, client keyboard and mouse are dropped");
                    InputSink::Missing
                }
            };
            run(&mut rx, &mut sink, &inbox, &|| stop.is_stopped());
        })?;
    let control = InputControl::new(commands, handle.thread().clone());
    Ok((control, handle))
}
