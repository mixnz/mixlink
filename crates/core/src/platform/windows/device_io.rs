//! Opening our drivers' device interfaces and sending them buffered IOCTLs (`vdisplay` spec 0015,
//! `vhid` spec 0016). Callers map the errors to their own types.

use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};

use windows::Win32::Devices::DeviceAndDriverInstallation::{
    CM_GET_DEVICE_INTERFACE_LIST_PRESENT, CM_Get_Device_Interface_List_SizeW,
    CM_Get_Device_Interface_ListW, CR_SUCCESS,
};
use windows::Win32::Foundation::{GENERIC_READ, GENERIC_WRITE, HANDLE};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::Win32::System::IO::DeviceIoControl;
use windows::core::{GUID, PCWSTR};

/// Opens the first present interface of `interface` for read + write. `Ok(None)`: no such interface
/// (driver not installed or device not started).
pub fn open_interface(interface: &GUID) -> windows::core::Result<Option<OwnedHandle>> {
    let Some(path) = interface_path(interface) else {
        return Ok(None);
    };
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
    }?;
    // SAFETY: CreateFileW returned a valid handle that nothing else owns.
    Ok(Some(unsafe { OwnedHandle::from_raw_handle(handle.0) }))
}

/// Synchronous `DeviceIoControl`; returns the bytes written to `output`.
pub fn ioctl(
    handle: &OwnedHandle,
    code: u32,
    input: &[u8],
    output: &mut [u8],
) -> windows::core::Result<usize> {
    let mut returned = 0u32;
    let in_len = u32::try_from(input.len()).unwrap_or(u32::MAX);
    let out_len = u32::try_from(output.len()).unwrap_or(u32::MAX);
    let out_ptr = (!output.is_empty()).then(|| output.as_mut_ptr().cast());
    // SAFETY: synchronous call on an open handle; both buffers outlive it and their lengths are exact.
    unsafe {
        DeviceIoControl(
            HANDLE(handle.as_raw_handle()),
            code,
            Some(input.as_ptr().cast()),
            in_len,
            out_ptr,
            out_len,
            Some(&raw mut returned),
            None,
        )
    }?;
    Ok(usize::try_from(returned).unwrap_or(0))
}

/// First present device interface path of `interface`, NUL-terminated.
fn interface_path(interface: &GUID) -> Option<Vec<u16>> {
    let mut len = 0u32;
    // SAFETY: `len` is a valid out pointer; the GUID outlives the call.
    let got = unsafe {
        CM_Get_Device_Interface_List_SizeW(
            &raw mut len,
            interface,
            PCWSTR::null(),
            CM_GET_DEVICE_INTERFACE_LIST_PRESENT,
        )
    };
    if got != CR_SUCCESS || len <= 1 {
        return None;
    }
    let mut list = vec![0u16; usize::try_from(len).unwrap_or(0)];
    // SAFETY: `list` holds `len` characters as the size call asked for.
    let got = unsafe {
        CM_Get_Device_Interface_ListW(
            interface,
            PCWSTR::null(),
            &mut list,
            CM_GET_DEVICE_INTERFACE_LIST_PRESENT,
        )
    };
    if got != CR_SUCCESS {
        return None;
    }
    let end = list.iter().position(|&c| c == 0).unwrap_or(0);
    if end == 0 {
        return None;
    }
    list.truncate(end + 1);
    Some(list)
}
