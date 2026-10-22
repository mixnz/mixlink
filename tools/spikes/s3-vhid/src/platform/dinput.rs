//! DirectInput 8: which game controllers it lists (case B).

use windows::Win32::Devices::HumanInterfaceDevice::{
    DI8DEVCLASS_GAMECTRL, DIDEVICEINSTANCEW, DIEDFL_ATTACHEDONLY, DIRECTINPUT_VERSION,
    DirectInput8Create, IDirectInput8W,
};
use windows::Win32::Foundation::HINSTANCE;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::core::{BOOL, Interface};

use super::os_err;
use crate::feed::FeedError;

#[derive(Clone, Debug)]
pub struct DiDevice {
    pub name: String,
    /// `guidProduct.Data1` = PID << 16 | VID for HID devices.
    pub vid: u16,
    pub pid: u16,
}

unsafe extern "system" fn on_device(
    instance: *mut DIDEVICEINSTANCEW,
    context: *mut core::ffi::c_void,
) -> BOOL {
    // SAFETY: DirectInput passes a valid instance; `context` is the Vec from `game_controllers`.
    let (instance, out) = unsafe { (&*instance, &mut *(context as *mut Vec<DiDevice>)) };
    let end = instance
        .tszProductName
        .iter()
        .position(|&c| c == 0)
        .unwrap_or(260);
    let data1 = instance.guidProduct.data1;
    out.push(DiDevice {
        name: String::from_utf16_lossy(&instance.tszProductName[..end]),
        vid: (data1 & 0xFFFF) as u16,
        pid: (data1 >> 16) as u16,
    });
    BOOL(1) // DIENUM_CONTINUE
}

pub fn game_controllers() -> Result<Vec<DiDevice>, FeedError> {
    // SAFETY: null module name = this executable.
    let module = unsafe { GetModuleHandleW(None) }.map_err(|e| os_err("GetModuleHandleW", e))?;
    let mut raw = std::ptr::null_mut();
    // SAFETY: valid IID and out pointer; no aggregation.
    unsafe {
        DirectInput8Create(
            HINSTANCE(module.0),
            DIRECTINPUT_VERSION,
            &IDirectInput8W::IID,
            &mut raw,
            None,
        )
    }
    .map_err(|e| os_err("DirectInput8Create", e))?;
    // SAFETY: DirectInput8Create returned an owned IDirectInput8W pointer.
    let di = unsafe { IDirectInput8W::from_raw(raw) };
    let mut out: Vec<DiDevice> = Vec::new();
    // SAFETY: callback matches LPDIENUMDEVICESCALLBACKW; `out` outlives the call.
    unsafe {
        di.EnumDevices(
            DI8DEVCLASS_GAMECTRL,
            Some(on_device),
            (&mut out as *mut Vec<DiDevice>).cast(),
            DIEDFL_ATTACHEDONLY,
        )
    }
    .map_err(|e| os_err("IDirectInput8::EnumDevices", e))?;
    Ok(out)
}
