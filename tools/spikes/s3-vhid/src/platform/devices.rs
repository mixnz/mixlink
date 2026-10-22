//! HID top-level collections seen by Raw Input (all of them, incl. vendor-defined).

use windows::Win32::Foundation::HANDLE;
use windows::Win32::UI::Input::{
    GetRawInputDeviceInfoW, GetRawInputDeviceList, RAWINPUTDEVICELIST, RID_DEVICE_INFO,
    RIDI_DEVICEINFO, RIDI_DEVICENAME, RIM_TYPEHID, RIM_TYPEKEYBOARD, RIM_TYPEMOUSE,
};

use crate::feed::FeedError;

#[derive(Clone, Debug)]
pub struct RawDevice {
    pub handle: isize,
    /// Device interface path, usable with `CreateFileW`.
    pub path: String,
    /// "keyboard", "mouse" or "hid".
    pub kind: &'static str,
    pub usage_page: u16,
    pub usage: u16,
}

impl RawDevice {
    /// True for a collection of our driver (`VID_DEED&PID_FEED`).
    pub fn is_vhid(&self) -> bool {
        self.path.to_ascii_uppercase().contains("VID_DEED&PID_FEED")
    }

    /// Collection number from the path (`&COL05` -> 5), if any.
    pub fn collection(&self) -> Option<u8> {
        let upper = self.path.to_ascii_uppercase();
        let at = upper.find("&COL")?;
        upper.get(at + 4..at + 6)?.parse().ok()
    }
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
    Some(RawDevice {
        handle: handle.0 as isize,
        path,
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

/// Raw Input handle of our collection `col` (1..=4), if present.
pub fn vhid_handle(devices: &[RawDevice], col: u8) -> Option<isize> {
    devices
        .iter()
        .find(|d| d.is_vhid() && d.collection() == Some(col))
        .map(|d| d.handle)
}
