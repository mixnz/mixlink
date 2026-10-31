//! Raw Input for the keyboard is off in the stream process (spec 0021). While a process has the
//! keyboard registered for Raw Input and one of its windows is in front, Windows never calls that
//! process's low-level keyboard hook (and starves the older hooks of other programs with it); found on
//! the client and reproduced with a plain window. winit registers mouse and keyboard; the mouse stays.

use windows::Win32::UI::Input::{
    GetRegisteredRawInputDevices, RAWINPUTDEVICE, RIDEV_REMOVE, RegisterRawInputDevices,
};

const USAGE_PAGE_GENERIC: u16 = 1;
const USAGE_KEYBOARD: u16 = 6;
/// Raw input registrations read back at most (a process has a handful).
const MAX_DEVICES: usize = 16;

/// Removes this process's keyboard Raw Input registration, if any.
pub fn stop_raw_keyboard() -> windows::core::Result<()> {
    if !registered_usages().contains(&USAGE_KEYBOARD) {
        return Ok(());
    }
    let remove = RAWINPUTDEVICE {
        usUsagePage: USAGE_PAGE_GENERIC,
        usUsage: USAGE_KEYBOARD,
        dwFlags: RIDEV_REMOVE,
        hwndTarget: Default::default(),
    };
    // SAFETY: one valid entry; `RIDEV_REMOVE` needs a null target, as given.
    unsafe { RegisterRawInputDevices(&[remove], size_of::<RAWINPUTDEVICE>() as u32) }
}

/// Generic-desktop usages this process has registered for Raw Input, sorted.
pub fn registered_usages() -> Vec<u16> {
    let mut devices = [RAWINPUTDEVICE::default(); MAX_DEVICES];
    let mut count = MAX_DEVICES as u32;
    // SAFETY: `devices` holds `count` entries of the declared size.
    let got = unsafe {
        GetRegisteredRawInputDevices(
            Some(devices.as_mut_ptr()),
            &mut count,
            size_of::<RAWINPUTDEVICE>() as u32,
        )
    };
    if got == u32::MAX {
        return Vec::new();
    }
    let mut usages: Vec<u16> = devices
        .iter()
        .take(got as usize)
        .filter(|d| d.usUsagePage == USAGE_PAGE_GENERIC)
        .map(|d| d.usUsage)
        .collect();
    usages.sort_unstable();
    usages
}

#[cfg(test)]
#[path = "raw_keyboard_tests.rs"]
mod tests;
