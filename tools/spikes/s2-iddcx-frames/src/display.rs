//! Finds the MixLink virtual monitor (root-enumerated IddCx adapter, `ROOT\DISPLAY\0000`).

pub fn is_virtual_adapter_path(path: &str) -> bool {
    path.to_ascii_uppercase().contains(r"\ROOT#DISPLAY#")
}

pub fn from_wide(w: &[u16]) -> String {
    let end = w.iter().position(|&c| c == 0).unwrap_or(w.len());
    String::from_utf16_lossy(&w[..end])
}

/// Refresh rate every S2b round must run at (spec 0005).
pub const REQUIRED_HZ: u32 = 120;

/// `s2-ctl check`: Ok when the virtual monitor is the main display at 120 Hz (spec 0004: composition
/// rate and lock/login follow the main display), else Err with what to fix.
pub fn check_verdict(
    main: &str,
    main_hz: u32,
    virtual_gdi: Option<&str>,
) -> Result<String, String> {
    let Some(virt) = virtual_gdi else {
        return Err(format!(
            "main {main} {main_hz} Hz, no virtual monitor: install the driver and enable the monitor"
        ));
    };
    let is_main = virt.eq_ignore_ascii_case(main);
    let line = format!(
        "main {main} {main_hz} Hz, virtual {virt} {}",
        if is_main { "is main" } else { "is not main" }
    );
    if !is_main {
        return Err(format!("{line}: make the virtual monitor the main display"));
    }
    if main_hz != REQUIRED_HZ {
        return Err(format!("{line}: set it to {REQUIRED_HZ} Hz"));
    }
    Ok(line)
}

/// GDI name and refresh rate of the main (primary) display.
#[cfg(windows)]
pub fn main_display() -> Option<(String, u32)> {
    use std::mem::size_of;
    use windows::Win32::Graphics::Gdi::{
        DISPLAY_DEVICE_PRIMARY_DEVICE, DISPLAY_DEVICEW, EnumDisplayDevicesW,
    };
    use windows::core::PCWSTR;

    for i in 0.. {
        let mut dev = DISPLAY_DEVICEW {
            cb: size_of::<DISPLAY_DEVICEW>() as u32,
            ..Default::default()
        };
        // SAFETY: `dev.cb` is set; null device name enumerates adapters' outputs.
        if !unsafe { EnumDisplayDevicesW(PCWSTR::null(), i, &mut dev, 0) }.as_bool() {
            return None;
        }
        if dev.StateFlags & DISPLAY_DEVICE_PRIMARY_DEVICE == DISPLAY_DEVICE_PRIMARY_DEVICE {
            let name = from_wide(&dev.DeviceName);
            let hz = current_mode(&name)?.dmDisplayFrequency;
            return Some((name, hz));
        }
    }
    None
}

#[cfg(windows)]
fn current_mode(gdi: &str) -> Option<windows::Win32::Graphics::Gdi::DEVMODEW> {
    use std::mem::size_of;
    use windows::Win32::Graphics::Gdi::{DEVMODEW, ENUM_CURRENT_SETTINGS, EnumDisplaySettingsW};
    use windows::core::HSTRING;

    let mut mode = DEVMODEW {
        dmSize: size_of::<DEVMODEW>() as u16,
        ..Default::default()
    };
    // SAFETY: `dmSize` is set; the name outlives the call.
    unsafe { EnumDisplaySettingsW(&HSTRING::from(gdi), ENUM_CURRENT_SETTINGS, &mut mode) }
        .as_bool()
        .then_some(mode)
}

/// Sets the refresh rate of display `gdi` (same resolution), not saved to the registry.
#[cfg(windows)]
pub fn set_refresh(gdi: &str, hz: u32) -> Result<(), String> {
    use windows::Win32::Graphics::Gdi::{
        CDS_TYPE, ChangeDisplaySettingsExW, DISP_CHANGE_SUCCESSFUL, DM_DISPLAYFREQUENCY,
    };
    use windows::core::HSTRING;

    let mut mode = current_mode(gdi).ok_or(format!("no current mode for {gdi}"))?;
    mode.dmDisplayFrequency = hz;
    mode.dmFields = DM_DISPLAYFREQUENCY;
    // SAFETY: `mode` came from EnumDisplaySettingsW and lives for the call.
    let result = unsafe {
        ChangeDisplaySettingsExW(&HSTRING::from(gdi), Some(&mode), None, CDS_TYPE(0), None)
    };
    if result == DISP_CHANGE_SUCCESSFUL {
        Ok(())
    } else {
        Err(format!("{gdi} to {hz} Hz failed: DISP_CHANGE {}", result.0))
    }
}

/// GDI name (`\\.\DISPLAYn`) of the active virtual monitor, if any.
#[cfg(windows)]
pub fn virtual_gdi_name() -> Option<String> {
    use std::mem::size_of;
    use windows::Win32::Devices::Display::{
        DISPLAYCONFIG_ADAPTER_NAME, DISPLAYCONFIG_DEVICE_INFO_GET_ADAPTER_NAME,
        DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME, DISPLAYCONFIG_MODE_INFO,
        DISPLAYCONFIG_PATH_INFO, DISPLAYCONFIG_SOURCE_DEVICE_NAME, DisplayConfigGetDeviceInfo,
        GetDisplayConfigBufferSizes, QDC_ONLY_ACTIVE_PATHS, QueryDisplayConfig,
    };

    let (mut n_paths, mut n_modes) = (0u32, 0u32);
    // SAFETY: out-pointers are valid locals.
    unsafe { GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut n_paths, &mut n_modes) }
        .ok()
        .ok()?;
    let mut paths = vec![DISPLAYCONFIG_PATH_INFO::default(); n_paths as usize];
    let mut modes = vec![DISPLAYCONFIG_MODE_INFO::default(); n_modes as usize];
    // SAFETY: buffers hold exactly the element counts passed in.
    unsafe {
        QueryDisplayConfig(
            QDC_ONLY_ACTIVE_PATHS,
            &mut n_paths,
            paths.as_mut_ptr(),
            &mut n_modes,
            modes.as_mut_ptr(),
            None,
        )
    }
    .ok()
    .ok()?;

    for path in &paths[..n_paths as usize] {
        let mut adapter = DISPLAYCONFIG_ADAPTER_NAME::default();
        adapter.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_ADAPTER_NAME;
        adapter.header.size = size_of::<DISPLAYCONFIG_ADAPTER_NAME>() as u32;
        adapter.header.adapterId = path.targetInfo.adapterId;
        // SAFETY: header type and size match the struct passed.
        if unsafe { DisplayConfigGetDeviceInfo(&mut adapter.header) } != 0 {
            continue;
        }
        if !is_virtual_adapter_path(&from_wide(&adapter.adapterDevicePath)) {
            continue;
        }
        let mut source = DISPLAYCONFIG_SOURCE_DEVICE_NAME::default();
        source.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME;
        source.header.size = size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32;
        source.header.adapterId = path.sourceInfo.adapterId;
        source.header.id = path.sourceInfo.id;
        // SAFETY: header type and size match the struct passed.
        if unsafe { DisplayConfigGetDeviceInfo(&mut source.header) } == 0 {
            return Some(from_wide(&source.viewGdiDeviceName));
        }
    }
    None
}

#[cfg(test)]
#[path = "display_tests.rs"]
mod tests;
