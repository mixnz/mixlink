//! Virtual Xbox pad nodes (spec 0016): one root node per slot with an explicit instance ID
//! (`input::pad_ids`), bound to `vhid.inf`'s pad model from the driver store.

use windows::Win32::Devices::DeviceAndDriverInstallation::{
    DIF_REGISTERDEVICE, DIINSTALLDEVICE_FLAGS, DiInstallDevice, SETUP_DI_DEVICE_CREATION_FLAGS,
    SP_DEVINFO_DATA, SPDRP_HARDWAREID, SetupDiCallClassInstaller, SetupDiCreateDeviceInfoList,
    SetupDiCreateDeviceInfoW, SetupDiOpenDeviceInfoW, SetupDiSetDevicePropertyW,
    SetupDiSetDeviceRegistryPropertyW,
};
use windows::Win32::Devices::Properties::{DEVPKEY_Device_ContainerId, DEVPROP_TYPE_GUID};
use windows::Win32::Foundation::ERROR_NO_SUCH_DEVINST;
use windows::core::{GUID, HSTRING};

use super::devices::{DevInfo, class_guid, devinfo_data, multi_sz, remove_node};
use crate::drivers::flow::RemoveOutcome;
use crate::drivers::package::{DRIVERS, DriverSpec};
use crate::error::SvcError;
use crate::input::pad_ids::{PAD_HARDWARE_ID, pad_container_guid, pad_instance_id, valid_slot};

const PAD_DESCRIPTION: &str = "MixLink Virtual Xbox Controller";

/// Creates the pad node of `slot` and installs the driver from the store. `false` when it exists.
pub fn create_pad_node(slot: u8) -> Result<bool, SvcError> {
    if !valid_slot(slot) {
        return Err(SvcError::Usage(format!("pad slot {slot} is out of range")));
    }
    let guid = class_guid(vhid_spec()?)?;
    let instance = HSTRING::from(pad_instance_id(slot));
    if open_node(&instance)?.is_some() {
        return Ok(false);
    }
    // SAFETY: the GUID outlives the call; no window.
    let set = unsafe { SetupDiCreateDeviceInfoList(Some(&guid), None) }
        .map_err(SvcError::win32("SetupDiCreateDeviceInfoList"))?;
    let set = DevInfo(set);
    let mut data = devinfo_data();
    // SAFETY: valid set; full instance ID (no DICD_GENERATE_ID); strings and GUID outlive the call.
    unsafe {
        SetupDiCreateDeviceInfoW(
            set.0,
            &instance,
            &guid,
            &HSTRING::from(PAD_DESCRIPTION),
            None,
            SETUP_DI_DEVICE_CREATION_FLAGS(0),
            Some(&mut data),
        )
    }
    .map_err(SvcError::win32("SetupDiCreateDeviceInfoW(pad)"))?;
    let ids = multi_sz(PAD_HARDWARE_ID);
    // SAFETY: plain byte view of `ids`, which outlives the call.
    let bytes = unsafe {
        std::slice::from_raw_parts(ids.as_ptr().cast::<u8>(), size_of_val(ids.as_slice()))
    };
    // SAFETY: valid set and element; `bytes` is a REG_MULTI_SZ.
    unsafe { SetupDiSetDeviceRegistryPropertyW(set.0, &mut data, SPDRP_HARDWAREID, Some(bytes)) }
        .map_err(SvcError::win32("SetupDiSetDeviceRegistryPropertyW(pad)"))?;
    // SAFETY: valid set and element with its hardware ID set.
    unsafe { SetupDiCallClassInstaller(DIF_REGISTERDEVICE, set.0, Some(&data)) }
        .map_err(SvcError::win32("SetupDiCallClassInstaller(pad)"))?;
    set_container(&set, &data, slot);
    // SAFETY: valid set and registered element; best driver from the store, no UI, no reboot query.
    let installed =
        unsafe { DiInstallDevice(None, set.0, &data, None, DIINSTALLDEVICE_FLAGS(0), None) };
    if let Err(error) = installed {
        let _ = remove_node(&set, &data);
        return Err(SvcError::win32("DiInstallDevice(pad)")(error));
    }
    Ok(true)
}

/// Removes the pad node of `slot`; `NoDevice` when there is none.
pub fn remove_pad_node(slot: u8) -> Result<RemoveOutcome, SvcError> {
    let instance = HSTRING::from(pad_instance_id(slot));
    match open_node(&instance)? {
        Some((set, data)) => remove_node(&set, &data),
        None => Ok(RemoveOutcome::NoDevice),
    }
}

/// Own container per slot: with the null container XInput may skip a slot (spec 0016). Best effort:
/// the pad still works without it, so a refusal is only logged.
fn set_container(set: &DevInfo, data: &SP_DEVINFO_DATA, slot: u8) {
    let guid = GUID::from_u128(pad_container_guid(slot));
    // SAFETY: plain byte view of a GUID local that outlives the call.
    let bytes =
        unsafe { std::slice::from_raw_parts((&raw const guid).cast::<u8>(), size_of::<GUID>()) };
    // SAFETY: valid set and element; `bytes` is a DEVPROP_TYPE_GUID value.
    let set_result = unsafe {
        SetupDiSetDevicePropertyW(
            set.0,
            data,
            &DEVPKEY_Device_ContainerId,
            DEVPROP_TYPE_GUID,
            Some(bytes),
            0,
        )
    };
    if let Err(error) = set_result {
        log::warn!("pad {slot}: container ID not set: {error}");
    }
}

fn vhid_spec() -> Result<&'static DriverSpec, SvcError> {
    DRIVERS
        .iter()
        .find(|spec| spec.name == "vhid")
        .ok_or(SvcError::Driver {
            step: "pad",
            detail: "vhid package is not known".into(),
        })
}

/// The node with `instance` (present or not), or `None`.
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
