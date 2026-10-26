//! Opening the `vhid` driver as its feeder (spec 0016).

use mixlink_input::hid::report::{Report, Target, neutral_reports};
use mixlink_input::platform::windows::vhid::{VHid, VHidError};

/// Opens the driver and becomes its feeder at once by feeding neutral reports, so no other program
/// can feed while this handle is open (the driver's owner is the first handle that feeds). Only
/// nodes that exist are fed: a pad slot with no node would refuse the report.
pub fn open_feeder() -> Result<VHid, VHidError> {
    let vhid = VHid::open()?;
    let pads_present = vhid.query()?.pads_present;
    for report in &claim_reports(pads_present) {
        vhid.feed(report)?;
    }
    Ok(vhid)
}

/// Neutral reports of the keyboard / mouse node and of the pads in `pads_present` (bit per slot).
pub fn claim_reports(pads_present: u8) -> Vec<Report> {
    neutral_reports()
        .into_iter()
        .filter(|report| match report.target() {
            Target::Pad(slot) => pads_present & (1 << slot) != 0,
            Target::Keyboard | Target::MouseRel | Target::MouseAbs => true,
        })
        .collect()
}

#[cfg(test)]
#[path = "feeder_tests.rs"]
mod tests;
