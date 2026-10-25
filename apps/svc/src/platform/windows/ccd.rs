//! Raw CCD calls (QueryDisplayConfig / SetDisplayConfig / target names) for the display layout (spec 0015).

use windows::Win32::Devices::Display::{
    DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME, DISPLAYCONFIG_DEVICE_INFO_HEADER,
    DISPLAYCONFIG_MODE_INFO, DISPLAYCONFIG_PATH_INFO, DISPLAYCONFIG_TARGET_DEVICE_NAME,
    DisplayConfigGetDeviceInfo, GetDisplayConfigBufferSizes, QUERY_DISPLAY_CONFIG_FLAGS,
    QueryDisplayConfig, SET_DISPLAY_CONFIG_FLAGS, SetDisplayConfig,
};
use windows::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, ERROR_SUCCESS, LUID};

use crate::error::SvcError;

/// Retries when a display changes between the size call and the query.
const QUERY_ATTEMPTS: usize = 5;

/// Paths and modes as QueryDisplayConfig returned them.
#[derive(Clone, Default)]
pub struct Config {
    pub paths: Vec<DISPLAYCONFIG_PATH_INFO>,
    pub modes: Vec<DISPLAYCONFIG_MODE_INFO>,
}

impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("paths", &self.paths.len())
            .field("modes", &self.modes.len())
            .finish()
    }
}

/// What Windows knows about a target's monitor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetName {
    pub monitor_path: String,
    /// EDID bytes 8..10 as Windows reports them (see `MXL_MFG_ID` in `display_config.rs`).
    pub edid_manufacture_id: u16,
    pub edid_product_code_id: u16,
}

pub fn query(flags: QUERY_DISPLAY_CONFIG_FLAGS) -> Result<Config, SvcError> {
    let mut last = 0;
    for _ in 0..QUERY_ATTEMPTS {
        let (mut path_count, mut mode_count) = (0u32, 0u32);
        // SAFETY: both counts are valid out pointers.
        let got =
            unsafe { GetDisplayConfigBufferSizes(flags, &raw mut path_count, &raw mut mode_count) };
        if got != ERROR_SUCCESS {
            return Err(SvcError::Topology {
                step: "GetDisplayConfigBufferSizes",
                code: got.0.cast_signed(),
            });
        }
        let mut config = Config {
            paths: vec![DISPLAYCONFIG_PATH_INFO::default(); path_count as usize],
            modes: vec![DISPLAYCONFIG_MODE_INFO::default(); mode_count as usize],
        };
        // SAFETY: the arrays hold the counts passed with them; the call lowers the counts it used.
        let got = unsafe {
            QueryDisplayConfig(
                flags,
                &raw mut path_count,
                config.paths.as_mut_ptr(),
                &raw mut mode_count,
                config.modes.as_mut_ptr(),
                None,
            )
        };
        if got == ERROR_SUCCESS {
            config.paths.truncate(path_count as usize);
            config.modes.truncate(mode_count as usize);
            return Ok(config);
        }
        last = got.0;
        if got != ERROR_INSUFFICIENT_BUFFER {
            break;
        }
    }
    Err(SvcError::Topology {
        step: "QueryDisplayConfig",
        code: last.cast_signed(),
    })
}

pub fn apply(
    paths: Option<&[DISPLAYCONFIG_PATH_INFO]>,
    modes: Option<&[DISPLAYCONFIG_MODE_INFO]>,
    flags: SET_DISPLAY_CONFIG_FLAGS,
    step: &'static str,
) -> Result<(), SvcError> {
    // SAFETY: slices are passed with their own lengths and are only read.
    let got = unsafe { SetDisplayConfig(paths, modes, flags) };
    if got == 0 {
        Ok(())
    } else {
        Err(SvcError::Topology { step, code: got })
    }
}

pub fn target_name(adapter: LUID, target_id: u32) -> Result<TargetName, SvcError> {
    let mut name = DISPLAYCONFIG_TARGET_DEVICE_NAME {
        header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
            r#type: DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
            size: u32::try_from(std::mem::size_of::<DISPLAYCONFIG_TARGET_DEVICE_NAME>())
                .unwrap_or(0),
            adapterId: adapter,
            id: target_id,
        },
        ..Default::default()
    };
    // SAFETY: the header names the full struct size and type; the struct outlives the call.
    let got = unsafe { DisplayConfigGetDeviceInfo(&raw mut name.header) };
    if got != 0 {
        return Err(SvcError::Topology {
            step: "DisplayConfigGetDeviceInfo",
            code: got,
        });
    }
    let path = &name.monitorDevicePath;
    let end = path.iter().position(|&c| c == 0).unwrap_or(path.len());
    Ok(TargetName {
        monitor_path: String::from_utf16_lossy(&path[..end]),
        edid_manufacture_id: name.edidManufactureId,
        edid_product_code_id: name.edidProductCodeId,
    })
}
