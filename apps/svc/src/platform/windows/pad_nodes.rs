//! Virtual Xbox pad nodes (spec 0016): one software device per slot (`input::pad_ids`), bound to
//! `vhid.inf`'s pad model from the driver store. A software device is the only node whose container ID
//! we can set; without one per slot XInput skips a user index.
//!
//! PnP keeps a `ParentPresent` software device outside its devnode and enumerates it again after a
//! plain devnode uninstall, so removal goes through the software device API: reopen, lifetime back to
//! the handle, close (an unplug: open handles cannot veto it).

use std::ffi::c_void;
use std::sync::mpsc::{SyncSender, sync_channel};
use std::time::Duration;

use mixlink_input::platform::windows::vhid::VHid;
use windows::Win32::Devices::DeviceAndDriverInstallation::{
    CM_LOCATE_DEVNODE_NORMAL, CM_Locate_DevNodeW, CR_SUCCESS, SP_DEVINFO_DATA,
    SetupDiCreateDeviceInfoList, SetupDiOpenDeviceInfoW,
};
use windows::Win32::Devices::Enumeration::Pnp::{
    HSWDEVICE, SW_DEVICE_CREATE_INFO, SW_DEVICE_LIFETIME, SWDeviceCapabilitiesDriverRequired,
    SWDeviceCapabilitiesRemovable, SWDeviceCapabilitiesSilentInstall, SWDeviceLifetimeHandle,
    SWDeviceLifetimeParentPresent, SwDeviceClose, SwDeviceCreate, SwDeviceSetLifetime,
};
use windows::Win32::Foundation::{ERROR_NO_SUCH_DEVINST, ERROR_TIMEOUT};
use windows::core::{GUID, HRESULT, HSTRING, PCWSTR};

use super::devices::{DevInfo, devinfo_data, multi_sz, uninstall_node};
use crate::drivers::flow::RemoveOutcome;
use crate::error::SvcError;
use crate::input::pad_ids::{
    PAD_ENUMERATOR, PAD_HARDWARE_ID, pad_container_guid, pad_instance_id, pad_instance_suffix,
    valid_slot,
};
use crate::input::pads::PadHost;

const PAD_DESCRIPTION: &str = "MixLink Virtual Xbox Controller";
const ROOT_PARENT: &str = r"HTREE\ROOT\0";
/// Covers the first driver install from the store; later creates take well under a second.
const CREATE_TIMEOUT: Duration = Duration::from_secs(30);
const GONE_TIMEOUT: Duration = Duration::from_secs(5);
const GONE_POLL: Duration = Duration::from_millis(50);

/// Creates the pad node of `slot` (driver from the store, kept after this process exits). `false`
/// when it is present.
pub fn create_pad_node(slot: u8) -> Result<bool, SvcError> {
    if !valid_slot(slot) {
        return Err(SvcError::Usage(format!("pad slot {slot} is out of range")));
    }
    if pad_present(slot) {
        return Ok(false);
    }
    with_software_device(slot, |device, created| {
        created?;
        set_lifetime(device, SWDeviceLifetimeParentPresent)
    })?;
    Ok(true)
}

/// Removes the pad node of `slot` and its devnode; `NoDevice` when there is none.
pub fn remove_pad_node(slot: u8) -> Result<RemoveOutcome, SvcError> {
    let instance = HSTRING::from(pad_instance_id(slot));
    if open_node(&instance)?.is_none() {
        return Ok(RemoveOutcome::NoDevice);
    }
    // Closing with the handle lifetime removes the device, created or not.
    with_software_device(slot, |device, _created| {
        set_lifetime(device, SWDeviceLifetimeHandle)
    })?;
    // PnP removes the device after the close returns.
    let mut waited = Duration::ZERO;
    while pad_present(slot) && waited < GONE_TIMEOUT {
        std::thread::sleep(GONE_POLL);
        waited += GONE_POLL;
    }
    if pad_present(slot) {
        return Err(SvcError::Driver {
            step: "pad",
            detail: format!("pad {slot} is still present after its removal"),
        });
    }
    // The devnode left behind (not present) goes too, so `pad_ids` stays free for the next create.
    if let Some((set, data)) = open_node(&instance)? {
        uninstall_node(&set, &data)?;
    }
    Ok(RemoveOutcome::Removed)
}

/// True when the pad node of `slot` is present (a removed devnode does not count).
fn pad_present(slot: u8) -> bool {
    let mut devinst = 0u32;
    // SAFETY: `devinst` is a live local; the ID outlives the call.
    let found = unsafe {
        CM_Locate_DevNodeW(
            &mut devinst,
            &HSTRING::from(pad_instance_id(slot)),
            CM_LOCATE_DEVNODE_NORMAL,
        )
    };
    found == CR_SUCCESS
}

/// Opens (creates, or reopens when PnP has it) the software device of `slot`, waits for PnP's result,
/// runs `use_device` with it and closes the handle.
fn with_software_device<T>(
    slot: u8,
    use_device: impl FnOnce(HSWDEVICE, Result<(), SvcError>) -> Result<T, SvcError>,
) -> Result<T, SvcError> {
    let instance = HSTRING::from(pad_instance_suffix(slot));
    let hardware_ids = multi_sz(PAD_HARDWARE_ID);
    let description = HSTRING::from(PAD_DESCRIPTION);
    let container = GUID::from_u128(pad_container_guid(slot));
    let flags = SWDeviceCapabilitiesRemovable.0
        | SWDeviceCapabilitiesSilentInstall.0
        | SWDeviceCapabilitiesDriverRequired.0;
    let info = SW_DEVICE_CREATE_INFO {
        cbSize: u32::try_from(size_of::<SW_DEVICE_CREATE_INFO>()).unwrap_or(u32::MAX),
        pszInstanceId: PCWSTR(instance.as_ptr()),
        pszzHardwareIds: PCWSTR(hardware_ids.as_ptr()),
        pContainerId: &raw const container,
        CapabilityFlags: u32::try_from(flags).unwrap_or(0),
        pszDeviceDescription: PCWSTR(description.as_ptr()),
        ..SW_DEVICE_CREATE_INFO::default()
    };
    let (sender, receiver) = sync_channel::<HRESULT>(1);
    // SAFETY: every string, the GUID and `sender` outlive the handle, which is closed below before
    // they drop; the callback only reads `sender` through the context pointer.
    let device = unsafe {
        SwDeviceCreate(
            &HSTRING::from(PAD_ENUMERATOR),
            &HSTRING::from(ROOT_PARENT),
            &info,
            None,
            Some(on_created),
            Some((&raw const sender).cast::<c_void>()),
        )
    }
    .map_err(SvcError::win32("SwDeviceCreate(pad)"))?;
    let created = receiver
        .recv_timeout(CREATE_TIMEOUT)
        .unwrap_or(ERROR_TIMEOUT.to_hresult())
        .ok()
        .map_err(SvcError::win32("SwDeviceCreate(pad) callback"));
    let used = use_device(device, created);
    // SAFETY: live handle, closed once; no callback runs after it returns. With the handle lifetime
    // closing removes the device.
    unsafe { SwDeviceClose(device) };
    used
}

fn set_lifetime(device: HSWDEVICE, lifetime: SW_DEVICE_LIFETIME) -> Result<(), SvcError> {
    // SAFETY: live handle from SwDeviceCreate, still open.
    unsafe { SwDeviceSetLifetime(device, lifetime) }
        .map_err(SvcError::win32("SwDeviceSetLifetime(pad)"))
}

/// Creation result from PnP, on a thread pool thread.
unsafe extern "system" fn on_created(
    _device: HSWDEVICE,
    result: HRESULT,
    context: *const c_void,
    _instance: PCWSTR,
) {
    // SAFETY: `context` is the `SyncSender` in `with_software_device`, alive until the handle closes.
    let sender = unsafe { &*context.cast::<SyncSender<HRESULT>>() };
    let _ = sender.try_send(result);
}

/// The devnode with `instance` (present or not), or `None`.
fn open_node(instance: &HSTRING) -> Result<Option<(DevInfo, SP_DEVINFO_DATA)>, SvcError> {
    // SAFETY: no class filter; no window.
    let set = unsafe { SetupDiCreateDeviceInfoList(None, None) }
        .map_err(SvcError::win32("SetupDiCreateDeviceInfoList"))?;
    let set = DevInfo(set);
    let mut data = devinfo_data();
    // SAFETY: valid set; the ID outlives the call; `data` has cbSize set.
    match unsafe { SetupDiOpenDeviceInfoW(set.0, instance, None, 0, Some(&mut data)) } {
        Ok(()) => Ok(Some((set, data))),
        Err(error) if error.code() == ERROR_NO_SUCH_DEVINST.to_hresult() => Ok(None),
        Err(error) => Err(SvcError::win32("SetupDiOpenDeviceInfoW(pad)")(error)),
    }
}

/// `input::pads::PadHost` over software devices and the driver's `QUERY`.
pub struct WindowsPadHost;

impl PadHost for WindowsPadHost {
    fn exists(&mut self, slot: u8) -> Result<bool, SvcError> {
        Ok(pad_present(slot))
    }

    fn create(&mut self, slot: u8) -> Result<(), SvcError> {
        create_pad_node(slot).map(|_| ())
    }

    fn remove(&mut self, slot: u8) -> Result<RemoveOutcome, SvcError> {
        remove_pad_node(slot)
    }

    fn present(&mut self) -> Result<u8, SvcError> {
        Ok(VHid::open()?.query()?.pads_present)
    }

    fn sleep(&mut self, duration: Duration) {
        std::thread::sleep(duration);
    }
}
