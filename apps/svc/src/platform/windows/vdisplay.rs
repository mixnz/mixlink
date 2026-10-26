//! Control channel to the `vdisplay` driver (spec 0015): device interface lookup and the three IOCTLs.

use std::os::windows::io::OwnedHandle;

use mixlink_core::device_io;
use windows::Win32::Foundation::{ERROR_ACCESS_DENIED, ERROR_NOT_READY};
use windows::core::GUID;

use crate::display::ports::DisplayDriver;
use crate::display::wire::{
    self, AddMonitor, IOCTL_ADD_MONITOR, IOCTL_QUERY, IOCTL_REMOVE_MONITOR, QUERY_SIZE, QueryReply,
};
use crate::error::SvcError;

/// `MIXLINK_VDISPLAY_INTERFACE` in `drivers/vdisplay/Control.h`.
const INTERFACE: GUID = GUID::from_u128(0xe798cdf0_5f9c_4eb5_a4f0_8d13ded2d748);

/// An open handle to the driver. Monitors it adds leave when it closes (spec 0015, owner rule).
#[derive(Debug)]
pub struct VDisplay {
    handle: OwnedHandle,
}

impl VDisplay {
    pub fn open() -> Result<VDisplay, SvcError> {
        let handle = device_io::open_interface(&INTERFACE)
            .map_err(|error| {
                if error.code() == ERROR_ACCESS_DENIED.to_hresult() {
                    SvcError::NotAdmin
                } else {
                    SvcError::Win32 {
                        call: "CreateFileW(vdisplay)",
                        source: error,
                    }
                }
            })?
            .ok_or(SvcError::DisplayDriverMissing)?;
        Ok(VDisplay { handle })
    }

    /// Sends `ADD_MONITOR` with `input` as is (also malformed input, for `display reject-check`).
    pub fn add_raw(&mut self, input: &[u8]) -> Result<u32, SvcError> {
        let mut out = [0u8; 4];
        self.ioctl(
            IOCTL_ADD_MONITOR,
            "DeviceIoControl(ADD_MONITOR)",
            input,
            &mut out,
        )?;
        Ok(wire::decode_add_reply(&out)?)
    }

    fn ioctl(
        &self,
        code: u32,
        call: &'static str,
        input: &[u8],
        output: &mut [u8],
    ) -> Result<usize, SvcError> {
        device_io::ioctl(&self.handle, code, input, output).map_err(|error| {
            if error.code() == ERROR_NOT_READY.to_hresult() {
                SvcError::DisplayNotReady
            } else {
                SvcError::Win32 {
                    call,
                    source: error,
                }
            }
        })
    }
}

impl DisplayDriver for VDisplay {
    fn add_monitor(&mut self, req: &AddMonitor) -> Result<u32, SvcError> {
        let input = req.encode()?;
        self.add_raw(&input)
    }

    fn remove_monitor(&mut self, id: u32) -> Result<(), SvcError> {
        let input = wire::encode_remove(id);
        self.ioctl(
            IOCTL_REMOVE_MONITOR,
            "DeviceIoControl(REMOVE_MONITOR)",
            &input,
            &mut [],
        )?;
        Ok(())
    }

    fn query(&mut self) -> Result<QueryReply, SvcError> {
        let input = wire::encode_query();
        let mut out = [0u8; QUERY_SIZE];
        let len = self.ioctl(IOCTL_QUERY, "DeviceIoControl(QUERY)", &input, &mut out)?;
        Ok(wire::decode_query(&out[..len.min(QUERY_SIZE)])?)
    }
}
