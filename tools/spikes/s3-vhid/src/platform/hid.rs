//! Feeds `drivers/vhid` through its vendor control collection (`&Col05`).

use windows::Win32::Devices::HumanInterfaceDevice::HidD_GetFeature;
use windows::Win32::Foundation::{CloseHandle, GENERIC_READ, GENERIC_WRITE, HANDLE};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    WriteFile,
};
use windows::core::HSTRING;

use super::{devices, os_err};
use crate::feed::{FeedError, Feeder};
use crate::input::{Input, KeyboardState};
use crate::report::{
    CONTROL_REPORT_ID, Device, control_report, mouse_abs_payload, mouse_rel_payload, pad_payload,
};

const CONTROL_COLLECTION: u8 = 5;

pub struct VhidFeeder {
    handle: HANDLE,
    keyboard: KeyboardState,
}

impl VhidFeeder {
    pub fn open() -> Result<Self, FeedError> {
        let path = devices::vhid_interface_path(CONTROL_COLLECTION)?.ok_or(FeedError::Os(
                "vhid control collection (Col05, VID DEED PID FEED) not found: is the driver installed?".into(),
            ))?;
        Self::open_path(&path)
    }

    pub fn open_path(path: &str) -> Result<Self, FeedError> {
        // SAFETY: path is a NUL-terminated HSTRING; no security attributes or template.
        let handle = unsafe {
            CreateFileW(
                &HSTRING::from(path),
                (GENERIC_READ | GENERIC_WRITE).0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                None,
                OPEN_EXISTING,
                FILE_FLAGS_AND_ATTRIBUTES(0),
                None,
            )
        }
        .map_err(|e| os_err(&format!("CreateFileW({path})"), e))?;
        Ok(Self {
            handle,
            keyboard: KeyboardState::default(),
        })
    }

    /// Writes one raw control output report (used to test the driver's validation).
    pub fn write_raw(&mut self, report: &[u8]) -> Result<(), FeedError> {
        // SAFETY: synchronous write from a live slice to an open handle.
        unsafe { WriteFile(self.handle, Some(report), None, None) }
            .map_err(|e| os_err("WriteFile", e))
    }

    /// Reports the driver dropped because its ring was full (feature report 5).
    pub fn read_dropped(&self) -> Result<u32, FeedError> {
        let mut buf = [0u8; 5];
        buf[0] = CONTROL_REPORT_ID;
        // SAFETY: `buf` is 5 bytes, the feature report length incl. the ID.
        if !unsafe { HidD_GetFeature(self.handle, buf.as_mut_ptr().cast(), buf.len() as u32) } {
            return Err(FeedError::Os(format!(
                "HidD_GetFeature: {}",
                windows::core::Error::from_thread()
            )));
        }
        Ok(u32::from_le_bytes([buf[1], buf[2], buf[3], buf[4]]))
    }
}

impl Feeder for VhidFeeder {
    // HOT PATH
    fn send(&mut self, input: &Input) -> Result<(), FeedError> {
        let report = match *input {
            Input::Key { usage, down } => {
                control_report(Device::Keyboard, &self.keyboard.apply(usage, down))?
            }
            Input::MouseMove { dx, dy } => {
                control_report(Device::MouseRel, &mouse_rel_payload(0, dx, dy, 0))?
            }
            Input::MouseAbs { x, y } => {
                control_report(Device::MouseAbs, &mouse_abs_payload(0, x, y))?
            }
            Input::Pad(state) => control_report(Device::Gamepad, &pad_payload(&state))?,
        };
        self.write_raw(&report)
    }
}

impl Drop for VhidFeeder {
    fn drop(&mut self) {
        // SAFETY: handle came from CreateFileW and is closed once.
        let _ = unsafe { CloseHandle(self.handle) };
    }
}
