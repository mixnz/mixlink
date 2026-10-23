//! HID top-level collections seen by Raw Input (all of them, incl. vendor-defined).

use windows::Win32::Devices::DeviceAndDriverInstallation::{
    DIGCF_DEVICEINTERFACE, DIGCF_PRESENT, HDEVINFO, SP_DEVICE_INTERFACE_DATA,
    SP_DEVICE_INTERFACE_DETAIL_DATA_W, SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInterfaces,
    SetupDiGetClassDevsW, SetupDiGetDeviceInterfaceDetailW,
};
use windows::Win32::Devices::HumanInterfaceDevice::{
    HIDD_ATTRIBUTES, HidD_GetAttributes, HidD_GetHidGuid,
};
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::Win32::UI::Input::{
    GetRawInputDeviceInfoW, GetRawInputDeviceList, RAWINPUTDEVICELIST, RID_DEVICE_INFO,
    RIDI_DEVICEINFO, RIDI_DEVICENAME, RIM_TYPEHID, RIM_TYPEKEYBOARD, RIM_TYPEMOUSE,
};

use super::os_err;
use crate::feed::FeedError;
use crate::ident;
use windows::core::HSTRING;

#[derive(Clone, Debug)]
pub struct RawDevice {
    pub handle: isize,
    /// Device interface path, usable with `CreateFileW`.
    pub path: String,
    /// "keyboard", "mouse" or "hid".
    pub kind: &'static str,
    pub usage_page: u16,
    pub usage: u16,
    /// From `HidD_GetAttributes`; 0 when the collection could not be opened.
    pub vid: u16,
    pub pid: u16,
}

impl RawDevice {
    /// True for a collection of our driver (by attributes: the root-enumerated path has no VID/PID).
    pub fn is_vhid(&self) -> bool {
        ident::is_vhid(self.vid, self.pid)
    }

    /// Collection number from the path (`&Col05` -> 5), if any.
    pub fn collection(&self) -> Option<u8> {
        ident::collection_from_path(&self.path)
    }
}

/// Vendor / product id of a HID collection. Opens it with no access rights, which works
/// even for keyboards and mice the system holds exclusively.
fn attributes(path: &str) -> Option<(u16, u16)> {
    // SAFETY: NUL-terminated path; zero access, no security attributes or template.
    let handle = unsafe {
        CreateFileW(
            &HSTRING::from(path),
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_FLAGS_AND_ATTRIBUTES(0),
            None,
        )
    }
    .ok()?;
    let mut attrs = HIDD_ATTRIBUTES {
        Size: std::mem::size_of::<HIDD_ATTRIBUTES>() as u32,
        ..Default::default()
    };
    // SAFETY: open handle and a sized HIDD_ATTRIBUTES out struct.
    let ok = unsafe { HidD_GetAttributes(handle, &mut attrs) };
    // SAFETY: handle from CreateFileW, closed once.
    let _ = unsafe { CloseHandle(handle) };
    ok.then_some((attrs.VendorID, attrs.ProductID))
}

pub fn list() -> Result<Vec<RawDevice>, FeedError> {
    let size = std::mem::size_of::<RAWINPUTDEVICELIST>() as u32;
    let mut count = 0u32;
    // SAFETY: null list + count pointer asks for the number of devices.
    if unsafe { GetRawInputDeviceList(None, &mut count, size) } == u32::MAX {
        return Err(FeedError::Os("GetRawInputDeviceList(count) failed".into()));
    }
    let mut raw = vec![RAWINPUTDEVICELIST::default(); count as usize];
    // SAFETY: `raw` holds `count` entries of `size` bytes.
    let got = unsafe { GetRawInputDeviceList(Some(raw.as_mut_ptr()), &mut count, size) };
    if got == u32::MAX {
        return Err(FeedError::Os("GetRawInputDeviceList failed".into()));
    }
    raw.truncate(got as usize);
    Ok(raw.iter().filter_map(|d| describe(d.hDevice)).collect())
}

fn describe(handle: HANDLE) -> Option<RawDevice> {
    let path = device_name(handle)?;
    let mut info = RID_DEVICE_INFO {
        cbSize: std::mem::size_of::<RID_DEVICE_INFO>() as u32,
        ..Default::default()
    };
    let mut size = info.cbSize;
    // SAFETY: `info` is a RID_DEVICE_INFO with cbSize set; `size` is its size.
    let r = unsafe {
        GetRawInputDeviceInfoW(
            Some(handle),
            RIDI_DEVICEINFO,
            Some((&mut info as *mut RID_DEVICE_INFO).cast()),
            &mut size,
        )
    };
    if r == u32::MAX {
        return None;
    }
    let (kind, usage_page, usage) = match info.dwType {
        RIM_TYPEKEYBOARD => ("keyboard", 1, 6),
        RIM_TYPEMOUSE => ("mouse", 1, 2),
        // SAFETY: dwType says the union holds `hid`.
        RIM_TYPEHID => unsafe {
            (
                "hid",
                info.Anonymous.hid.usUsagePage,
                info.Anonymous.hid.usUsage,
            )
        },
        _ => return None,
    };
    let (vid, pid) = attributes(&path).unwrap_or((0, 0));
    Some(RawDevice {
        handle: handle.0 as isize,
        path,
        vid,
        pid,
        kind,
        usage_page,
        usage,
    })
}

fn device_name(handle: HANDLE) -> Option<String> {
    let mut chars = 0u32;
    // SAFETY: null buffer asks for the name length in characters.
    unsafe { GetRawInputDeviceInfoW(Some(handle), RIDI_DEVICENAME, None, &mut chars) };
    if chars == 0 {
        return None;
    }
    let mut buf = vec![0u16; chars as usize];
    // SAFETY: `buf` holds `chars` UTF-16 units.
    let r = unsafe {
        GetRawInputDeviceInfoW(
            Some(handle),
            RIDI_DEVICENAME,
            Some(buf.as_mut_ptr().cast()),
            &mut chars,
        )
    };
    if r == u32::MAX {
        return None;
    }
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    Some(String::from_utf16_lossy(&buf[..end]))
}

/// Path of our collection `col` from the HID device interfaces (SetupDi). Raw Input does not
/// list vendor-defined collections, so the control collection (5) is found here.
pub fn vhid_interface_path(col: u8) -> Result<Option<String>, FeedError> {
    // SAFETY: no arguments; returns the HID interface class GUID.
    let guid = unsafe { HidD_GetHidGuid() };
    // SAFETY: valid GUID pointer; present device interfaces only.
    let set = unsafe {
        SetupDiGetClassDevsW(
            Some(&guid),
            None,
            None,
            DIGCF_PRESENT | DIGCF_DEVICEINTERFACE,
        )
    }
    .map_err(|e| os_err("SetupDiGetClassDevsW", e))?;
    let mut found = None;
    for index in 0.. {
        let mut data = SP_DEVICE_INTERFACE_DATA {
            cbSize: std::mem::size_of::<SP_DEVICE_INTERFACE_DATA>() as u32,
            ..Default::default()
        };
        // SAFETY: valid set, GUID and sized out struct; fails at the end of the list.
        if unsafe { SetupDiEnumDeviceInterfaces(set, None, &guid, index, &mut data) }.is_err() {
            break;
        }
        if let Some(path) = interface_path(set, &data)
            && ident::collection_from_path(&path) == Some(col)
            && attributes(&path).is_some_and(|(v, p)| ident::is_vhid(v, p))
        {
            found = Some(path);
            break;
        }
    }
    // SAFETY: set from SetupDiGetClassDevsW, destroyed once.
    let _ = unsafe { SetupDiDestroyDeviceInfoList(set) };
    Ok(found)
}

fn interface_path(set: HDEVINFO, data: &SP_DEVICE_INTERFACE_DATA) -> Option<String> {
    let mut required = 0u32;
    // SAFETY: null detail buffer asks for the required size.
    let _ =
        unsafe { SetupDiGetDeviceInterfaceDetailW(set, data, None, 0, Some(&mut required), None) };
    if (required as usize) < std::mem::size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() {
        return None;
    }
    // u32 storage keeps the detail struct aligned.
    let mut buf = vec![0u32; (required as usize).div_ceil(4)];
    let detail = buf.as_mut_ptr() as *mut SP_DEVICE_INTERFACE_DETAIL_DATA_W;
    // SAFETY: `buf` is at least `required` bytes and 4-byte aligned; cbSize is the fixed part's size.
    unsafe { (*detail).cbSize = std::mem::size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as u32 };
    // SAFETY: as above; the call writes at most `required` bytes.
    unsafe { SetupDiGetDeviceInterfaceDetailW(set, data, Some(detail), required, None, None) }
        .ok()?;
    let offset = std::mem::offset_of!(SP_DEVICE_INTERFACE_DETAIL_DATA_W, DevicePath);
    let units = (required as usize - offset) / 2;
    // SAFETY: DevicePath starts at `offset` and the buffer holds `units` UTF-16 units after it.
    let chars = unsafe {
        std::slice::from_raw_parts((buf.as_ptr() as *const u8).add(offset) as *const u16, units)
    };
    let end = chars.iter().position(|&c| c == 0).unwrap_or(units);
    Some(String::from_utf16_lossy(&chars[..end]))
}

/// Raw Input handle of our collection `col` (1..=4), if present.
pub fn vhid_handle(devices: &[RawDevice], col: u8) -> Option<isize> {
    devices
        .iter()
        .find(|d| d.is_vhid() && d.collection() == Some(col))
        .map(|d| d.handle)
}
