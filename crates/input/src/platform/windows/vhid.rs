//! Feeder for the `vhid` driver's control node (spec 0016): open its interface, send the IOCTLs.

use std::os::windows::io::OwnedHandle;

use mixlink_core::device_io;
use windows::Win32::Foundation::{
    ERROR_ACCESS_DENIED, ERROR_DEVICE_NOT_CONNECTED, ERROR_SHARING_VIOLATION,
};
use windows::core::GUID;

use super::vhid_wire::{self, IOCTL_FEED, IOCTL_QUERY, QUERY_SIZE, QueryReply, WireError};
use crate::hid::report::{Report, ReportSink};

/// `MIXLINK_VHID_INTERFACE` in `drivers/vhid/Control.h`.
const INTERFACE: GUID = GUID::from_u128(0x1ad24e8a_2048_46bd_b35c_bfad0cfefa8d);

#[derive(Debug, thiserror::Error)]
pub enum VHidError {
    #[error("the MixLink virtual input driver isn't installed")]
    Missing,
    #[error("only administrators can open the MixLink virtual input driver")]
    AccessDenied,
    #[error("another program is feeding the MixLink virtual input driver")]
    InUse,
    /// The control node cannot reach the HID node (gone, or in another WUDFHost process).
    #[error("the MixLink virtual input control node can't reach the input device")]
    NotPaired,
    #[error(transparent)]
    Wire(#[from] WireError),
    #[error("{call} failed: {source}")]
    Win32 {
        call: &'static str,
        #[source]
        source: windows::core::Error,
    },
}

/// An open handle to the control node. When the handle that fed reports closes, the driver
/// releases every key, button and stick (spec 0016, release-all).
#[derive(Debug)]
pub struct VHid {
    handle: OwnedHandle,
}

impl VHid {
    /// Opens the control node and checks the driver's wire version and report layouts.
    pub fn open() -> Result<VHid, VHidError> {
        let handle = device_io::open_interface(&INTERFACE)
            .map_err(|error| map_error("CreateFileW(vhid)", error))?
            .ok_or(VHidError::Missing)?;
        let vhid = VHid { handle };
        vhid.query()?;
        Ok(vhid)
    }

    pub fn query(&self) -> Result<QueryReply, VHidError> {
        let input = vhid_wire::encode_query();
        let mut out = [0u8; QUERY_SIZE];
        let len = device_io::ioctl(&self.handle, IOCTL_QUERY, &input, &mut out)
            .map_err(|error| map_error("DeviceIoControl(QUERY)", error))?;
        Ok(vhid_wire::decode_query(&out[..len.min(QUERY_SIZE)])?)
    }

    /// Sends one report. `InUse` when another handle is the feeder.
    // HOT PATH
    pub fn feed(&self, report: &Report) -> Result<(), VHidError> {
        let input = vhid_wire::encode_feed(report);
        device_io::ioctl(&self.handle, IOCTL_FEED, &input, &mut [])
            .map_err(|error| map_error("DeviceIoControl(FEED)", error))?;
        Ok(())
    }
}

impl ReportSink for VHid {
    type Error = VHidError;

    fn feed(&mut self, report: &Report) -> Result<(), VHidError> {
        VHid::feed(self, report)
    }
}

/// A shared feeder (`Arc<VHid>` in the service): `feed` only needs `&self`.
impl ReportSink for &VHid {
    type Error = VHidError;

    fn feed(&mut self, report: &Report) -> Result<(), VHidError> {
        VHid::feed(self, report)
    }
}

fn map_error(call: &'static str, error: windows::core::Error) -> VHidError {
    let code = error.code();
    if code == ERROR_ACCESS_DENIED.to_hresult() {
        VHidError::AccessDenied
    } else if code == ERROR_DEVICE_NOT_CONNECTED.to_hresult() {
        VHidError::NotPaired
    } else if code == ERROR_SHARING_VIOLATION.to_hresult() {
        VHidError::InUse
    } else {
        VHidError::Win32 {
            call,
            source: error,
        }
    }
}
