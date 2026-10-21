//! Finds the MixLink virtual monitor (root-enumerated IddCx adapter, `ROOT\DISPLAY\0000`).

pub fn is_virtual_adapter_path(path: &str) -> bool {
    path.to_ascii_uppercase().contains(r"\ROOT#DISPLAY#")
}

pub fn from_wide(w: &[u16]) -> String {
    let end = w.iter().position(|&c| c == 0).unwrap_or(w.len());
    String::from_utf16_lossy(&w[..end])
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
