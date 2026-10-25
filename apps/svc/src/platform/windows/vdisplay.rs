//! Control channel to the `vdisplay` driver (spec 0015): device interface lookup and the three IOCTLs.

use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};

use windows::Win32::Devices::DeviceAndDriverInstallation::{
    CM_GET_DEVICE_INTERFACE_LIST_PRESENT, CM_Get_Device_Interface_List_SizeW,
    CM_Get_Device_Interface_ListW, CR_SUCCESS,
};
use windows::Win32::Foundation::{
    ERROR_ACCESS_DENIED, ERROR_NOT_READY, GENERIC_READ, GENERIC_WRITE, HANDLE,
};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::Win32::System::IO::DeviceIoControl;
use windows::core::{GUID, PCWSTR};

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
        let path = interface_path()?;
        // SAFETY: `path` is NUL-terminated and outlives the call; no template handle.
        let handle = unsafe {
            CreateFileW(
                PCWSTR(path.as_ptr()),
                (GENERIC_READ | GENERIC_WRITE).0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                None,
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                None,
            )
        }
        .map_err(|error| {
            if error.code() == ERROR_ACCESS_DENIED.to_hresult() {
                SvcError::NotAdmin
            } else {
                SvcError::Win32 {
                    call: "CreateFileW(vdisplay)",
                    source: error,
                }
            }
        })?;
        // SAFETY: CreateFileW returned a valid handle that nothing else owns.
        let handle = unsafe { OwnedHandle::from_raw_handle(handle.0) };
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
        let mut returned = 0u32;
        let in_len = u32::try_from(input.len()).unwrap_or(u32::MAX);
        let out_len = u32::try_from(output.len()).unwrap_or(u32::MAX);
        let out_ptr = (!output.is_empty()).then(|| output.as_mut_ptr().cast());
        // SAFETY: synchronous call on our handle; both buffers outlive it and their lengths are exact.
        unsafe {
            DeviceIoControl(
                HANDLE(self.handle.as_raw_handle()),
                code,
                Some(input.as_ptr().cast()),
                in_len,
                out_ptr,
                out_len,
                Some(&raw mut returned),
                None,
            )
        }
        .map_err(|error| {
            if error.code() == ERROR_NOT_READY.to_hresult() {
                SvcError::DisplayNotReady
            } else {
                SvcError::Win32 {
                    call,
                    source: error,
                }
            }
        })?;
        Ok(usize::try_from(returned).unwrap_or(0))
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

/// First present device interface of the driver, NUL-terminated.
fn interface_path() -> Result<Vec<u16>, SvcError> {
    let mut len = 0u32;
    // SAFETY: `len` is a valid out pointer; the GUID is a static.
    let got = unsafe {
        CM_Get_Device_Interface_List_SizeW(
            &raw mut len,
            &INTERFACE,
            PCWSTR::null(),
            CM_GET_DEVICE_INTERFACE_LIST_PRESENT,
        )
    };
    if got != CR_SUCCESS || len <= 1 {
        return Err(SvcError::DisplayDriverMissing);
    }
    let mut list = vec![0u16; usize::try_from(len).unwrap_or(0)];
    // SAFETY: `list` holds `len` characters as the size call asked for.
    let got = unsafe {
        CM_Get_Device_Interface_ListW(
            &INTERFACE,
            PCWSTR::null(),
            &mut list,
            CM_GET_DEVICE_INTERFACE_LIST_PRESENT,
        )
    };
    if got != CR_SUCCESS {
        return Err(SvcError::DisplayDriverMissing);
    }
    let end = list.iter().position(|&c| c == 0).unwrap_or(0);
    if end == 0 {
        return Err(SvcError::DisplayDriverMissing);
    }
    list.truncate(end + 1);
    Ok(list)
}
