//! Virtual input (spec 0016): the `vhid` feeder the service holds and the `input ...` dev commands.

pub mod commands;
pub mod pad_ids;
pub mod padmap;
pub mod probe;
pub mod script;
pub mod status;

use mixlink_input::hid::report::neutral_reports;
use mixlink_input::platform::windows::vhid::{VHid, VHidError};

/// Opens the driver and becomes its feeder at once by feeding the neutral reports, so no other
/// program can feed while this handle is open (the driver's owner is the first handle that feeds).
pub fn open_feeder() -> Result<VHid, VHidError> {
    let vhid = VHid::open()?;
    for report in &neutral_reports() {
        vhid.feed(report)?;
    }
    Ok(vhid)
}
