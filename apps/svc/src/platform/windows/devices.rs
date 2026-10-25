//! Root device nodes for our hardware IDs (spec 0014): create one only after its package is in the
//! driver store (spec 0003 C), remove them without UI and report who holds one open (spec 0008).

use std::path::Path;

use windows::Win32::Devices::DeviceAndDriverInstallation::{
    CM_DEVNODE_STATUS_FLAGS, CM_Get_DevNode_Status, CM_PROB, CM_Query_And_Remove_SubTreeW,
    CM_REMOVE_NO_RESTART, CR_REMOVE_VETOED, CR_SUCCESS, DICD_GENERATE_ID, DIF_REGISTERDEVICE,
    DN_HAS_PROBLEM, DN_STARTED, DiUninstallDevice, HDEVINFO, INSTALLFLAG_NONINTERACTIVE,
    PNP_VETO_TYPE, PNP_VetoTypeUnknown, PNP_VetoWindowsApp, SETUP_DI_GET_CLASS_DEVS_FLAGS,
    SP_DEVINFO_DATA, SPDRP_HARDWAREID, SetupDiCallClassInstaller, SetupDiCreateDeviceInfoList,
    SetupDiCreateDeviceInfoW, SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInfo,
    SetupDiGetClassDevsW, SetupDiGetDeviceRegistryPropertyW, SetupDiSetDeviceRegistryPropertyW,
    UpdateDriverForPlugAndPlayDevicesW,
};
use windows::Win32::Foundation::HWND;
use windows::core::{GUID, HSTRING, PCWSTR};

use crate::drivers::flow::RemoveOutcome;
use crate::drivers::package::DriverSpec;
use crate::error::SvcError;

/// What `drivers status` reports for a device node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceState {
    /// No node with our hardware ID.
    Missing,
    /// A node exists but is not started (e.g. not present after a failed install).
    NotStarted,
    /// Started; `0` = working, else the device manager problem code.
    Problem(u32),
}

/// A `REG_MULTI_SZ` with one string.
pub fn multi_sz(s: &str) -> Vec<u16> {
    s.encode_utf16().chain([0, 0]).collect()
}

/// True when the `REG_MULTI_SZ` list holds `id` (case-insensitive, like PnP).
pub fn hardware_ids_match(list: &[u16], id: &str) -> bool {
    list.split(|&c| c == 0)
        .filter(|s| !s.is_empty())
        .any(|s| String::from_utf16_lossy(s).eq_ignore_ascii_case(id))
}

/// The holder's name when Windows reports an application (`PNP_VetoWindowsApp`), else `None`.
pub fn veto_app(kind: PNP_VETO_TYPE, name: &[u16]) -> Option<String> {
    if kind != PNP_VetoWindowsApp {
        return None;
    }
    let end = name.iter().position(|&c| c == 0).unwrap_or(name.len());
    let app = String::from_utf16_lossy(&name[..end]);
    (!app.is_empty()).then_some(app)
}

struct DevInfo(HDEVINFO);

impl Drop for DevInfo {
    fn drop(&mut self) {
        // SAFETY: the set came from SetupDiGetClassDevsW / SetupDiCreateDeviceInfoList, destroyed once.
        let _ = unsafe { SetupDiDestroyDeviceInfoList(self.0) };
    }
}

fn class_guid(spec: &DriverSpec) -> Result<GUID, SvcError> {
    GUID::try_from(spec.class_guid.trim_matches(['{', '}']))
        .map_err(SvcError::win32("GUID::try_from"))
}

fn devinfo_data() -> SP_DEVINFO_DATA {
    SP_DEVINFO_DATA {
        cbSize: u32::try_from(size_of::<SP_DEVINFO_DATA>()).unwrap_or(u32::MAX),
        ..SP_DEVINFO_DATA::default()
    }
}

/// Every node (present or not) of `spec`'s class whose hardware IDs hold `spec.hardware_id`.
fn find_devices(spec: &DriverSpec) -> Result<(DevInfo, Vec<SP_DEVINFO_DATA>), SvcError> {
    let guid = class_guid(spec)?;
    // SAFETY: the GUID outlives the call; no enumerator, no window; flags 0 = all nodes of the class.
    let set = unsafe {
        SetupDiGetClassDevsW(
            Some(&guid),
            PCWSTR::null(),
            None,
            SETUP_DI_GET_CLASS_DEVS_FLAGS(0),
        )
    }
    .map_err(SvcError::win32("SetupDiGetClassDevsW"))?;
    let set = DevInfo(set);
    let mut found = Vec::new();
    for index in 0u32.. {
        let mut data = devinfo_data();
        // SAFETY: valid set; `data` has cbSize set. Fails with ERROR_NO_MORE_ITEMS past the end.
        if unsafe { SetupDiEnumDeviceInfo(set.0, index, &mut data) }.is_err() {
            break;
        }
        let mut ids = [0u16; 512];
        // SAFETY: valid set and element; the byte view of `ids` is writable and sized.
        let read = unsafe {
            SetupDiGetDeviceRegistryPropertyW(
                set.0,
                &data,
                SPDRP_HARDWAREID,
                None,
                Some(std::slice::from_raw_parts_mut(
                    ids.as_mut_ptr().cast(),
                    size_of_val(&ids),
                )),
                None,
            )
        };
        if read.is_ok() && hardware_ids_match(&ids, spec.hardware_id) {
            found.push(data);
        }
    }
    Ok((set, found))
}

/// Creates the root node when none exists, then installs `inf`'s driver on every matching node.
pub fn ensure_device(spec: &DriverSpec, inf: &Path) -> Result<(), SvcError> {
    let (_, existing) = find_devices(spec)?;
    let created = if existing.is_empty() {
        Some(create_node(spec)?)
    } else {
        None
    };
    // SAFETY: the strings outlive the call; no window, non-interactive.
    let updated = unsafe {
        UpdateDriverForPlugAndPlayDevicesW(
            None,
            &HSTRING::from(spec.hardware_id),
            &HSTRING::from(inf.as_os_str()),
            INSTALLFLAG_NONINTERACTIVE,
            None,
        )
    };
    if let Err(error) = updated {
        if let Some((set, data)) = created {
            // Do not leave an empty node behind (spec 0003 C).
            // SAFETY: the set and element came from create_node.
            let _ = unsafe { DiUninstallDevice(HWND::default(), set.0, &data, 0, None) };
        }
        return Err(SvcError::win32("UpdateDriverForPlugAndPlayDevicesW")(error));
    }
    Ok(())
}

fn create_node(spec: &DriverSpec) -> Result<(DevInfo, SP_DEVINFO_DATA), SvcError> {
    let guid = class_guid(spec)?;
    // SAFETY: the GUID outlives the call; no window.
    let set = unsafe { SetupDiCreateDeviceInfoList(Some(&guid), None) }
        .map_err(SvcError::win32("SetupDiCreateDeviceInfoList"))?;
    let set = DevInfo(set);
    let mut data = devinfo_data();
    // SAFETY: valid set; the strings and GUID outlive the call; `data` has cbSize set.
    unsafe {
        SetupDiCreateDeviceInfoW(
            set.0,
            &HSTRING::from(spec.class_name),
            &guid,
            PCWSTR::null(),
            None,
            DICD_GENERATE_ID,
            Some(&mut data),
        )
    }
    .map_err(SvcError::win32("SetupDiCreateDeviceInfoW"))?;
    let ids = multi_sz(spec.hardware_id);
    // SAFETY: plain byte view of `ids`, which outlives the call.
    let bytes = unsafe {
        std::slice::from_raw_parts(ids.as_ptr().cast::<u8>(), size_of_val(ids.as_slice()))
    };
    // SAFETY: valid set and element; `bytes` is a REG_MULTI_SZ.
    unsafe { SetupDiSetDeviceRegistryPropertyW(set.0, &mut data, SPDRP_HARDWAREID, Some(bytes)) }
        .map_err(SvcError::win32("SetupDiSetDeviceRegistryPropertyW"))?;
    // SAFETY: valid set and element with its hardware ID set.
    unsafe { SetupDiCallClassInstaller(DIF_REGISTERDEVICE, set.0, Some(&data)) }
        .map_err(SvcError::win32("SetupDiCallClassInstaller"))?;
    Ok((set, data))
}

/// Removes every node of `spec`. Stops at the first node a program holds open.
pub fn remove_devices(spec: &DriverSpec) -> Result<RemoveOutcome, SvcError> {
    let (set, nodes) = find_devices(spec)?;
    if nodes.is_empty() {
        return Ok(RemoveOutcome::NoDevice);
    }
    for data in &nodes {
        let mut veto = PNP_VetoTypeUnknown;
        let mut name = [0u16; 260];
        // SAFETY: valid devnode; `veto` and `name` are live locals; no UI (no CM_REMOVE_UI_OK).
        let removed = unsafe {
            CM_Query_And_Remove_SubTreeW(
                data.DevInst,
                Some(&mut veto),
                Some(&mut name),
                CM_REMOVE_NO_RESTART,
            )
        };
        if removed == CR_REMOVE_VETOED {
            return Ok(RemoveOutcome::Held {
                app: veto_app(veto, &name),
            });
        }
        // Any other result (e.g. a node that is not present) still goes to DiUninstallDevice.
        // SAFETY: valid set and element from find_devices.
        unsafe { DiUninstallDevice(HWND::default(), set.0, data, 0, None) }
            .map_err(SvcError::win32("DiUninstallDevice"))?;
    }
    Ok(RemoveOutcome::Removed)
}

/// State of the first node with `spec`'s hardware ID.
pub fn device_state(spec: &DriverSpec) -> Result<DeviceState, SvcError> {
    let (_, nodes) = find_devices(spec)?;
    let Some(data) = nodes.first() else {
        return Ok(DeviceState::Missing);
    };
    let mut status = CM_DEVNODE_STATUS_FLAGS::default();
    let mut problem = CM_PROB::default();
    // SAFETY: valid devnode; out pointers are live locals.
    let got = unsafe { CM_Get_DevNode_Status(&mut status, &mut problem, data.DevInst, 0) };
    if got != CR_SUCCESS {
        return Ok(DeviceState::NotStarted);
    }
    if status.0 & DN_HAS_PROBLEM.0 != 0 {
        Ok(DeviceState::Problem(problem.0))
    } else if status.0 & DN_STARTED.0 != 0 {
        Ok(DeviceState::Problem(0))
    } else {
        Ok(DeviceState::NotStarted)
    }
}

#[cfg(test)]
#[path = "devices_tests.rs"]
mod tests;
