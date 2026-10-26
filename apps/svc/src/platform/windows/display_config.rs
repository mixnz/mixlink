//! `DisplayConfig` on the CCD API, in this process (spec 0015). Targets are matched by monitor
//! device path; adapter LUIDs are only used within one query.

use mixlink_core::config::Topology;
use mixlink_proto::control::DisplayInfo;
use windows::Win32::Devices::Display::{
    DISPLAYCONFIG_MODE_INFO, DISPLAYCONFIG_MODE_INFO_0, DISPLAYCONFIG_MODE_INFO_TYPE_SOURCE,
    DISPLAYCONFIG_PATH_INFO, DISPLAYCONFIG_PATH_TARGET_INFO, DISPLAYCONFIG_PIXELFORMAT_32BPP,
    DISPLAYCONFIG_RATIONAL, DISPLAYCONFIG_SCANLINE_ORDERING_PROGRESSIVE,
    DISPLAYCONFIG_SCANLINE_ORDERING_UNSPECIFIED, DISPLAYCONFIG_SOURCE_MODE, QDC_ALL_PATHS,
    QDC_ONLY_ACTIVE_PATHS, SDC_ALLOW_CHANGES, SDC_APPLY, SDC_SAVE_TO_DATABASE, SDC_TOPOLOGY_EXTEND,
    SDC_USE_SUPPLIED_DISPLAY_CONFIG,
};
use windows::Win32::Foundation::POINTL;
use windows::Win32::Graphics::Gdi::{
    DISPLAYCONFIG_PATH_ACTIVE, DISPLAYCONFIG_PATH_MODE_IDX_INVALID,
};

use super::ccd::{self, Config, TargetName};
use crate::display::edid::MXL_MFG;
use crate::display::ports::DisplayConfig;
use crate::display::topology::{RestorePlan, SNAPSHOT_VERSION, SavedTarget, Snapshot, plan_extend};
use crate::error::SvcError;

/// `MXL` as `DISPLAYCONFIG_TARGET_DEVICE_NAME::edidManufactureId` reports it (EDID bytes 8..10
/// read little-endian; checked against a live MixLink monitor, spec 0015 Results).
pub const MXL_MFG_ID: u16 = u16::from_le_bytes(MXL_MFG);

#[derive(Debug, Default)]
pub struct CcdConfig;

/// An active target that is a real monitor: with no monitor plugged in, Windows keeps one path
/// active (1024x768) whose target has no monitor device path; it must not count as a display,
/// or a headless host never gets its headless monitor. MixLink monitors only when `with_mixlink`.
fn counts_as_display(name: &TargetName, with_mixlink: bool) -> bool {
    !name.monitor_path.is_empty() && (with_mixlink || name.edid_manufacture_id != MXL_MFG_ID)
}

/// A MixLink monitor with this product code that Windows has finished setting up. Right after
/// `ADD_MONITOR` the target already reports the EDID ids but no monitor device path yet; a layout
/// cannot name it until the path is there.
fn is_ready_mixlink(name: &TargetName, product: u16) -> bool {
    name.edid_manufacture_id == MXL_MFG_ID
        && name.edid_product_code_id == product
        && !name.monitor_path.is_empty()
}

/// One display in a layout we apply.
struct Placement {
    monitor_path: String,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    refresh: DISPLAYCONFIG_RATIONAL,
}

fn source_mode(
    config: &Config,
    path: &DISPLAYCONFIG_PATH_INFO,
) -> Option<DISPLAYCONFIG_SOURCE_MODE> {
    // SAFETY: without QDC_VIRTUAL_MODE_AWARE the union holds a plain mode index.
    let index = unsafe { path.sourceInfo.Anonymous.modeInfoIdx };
    let mode = config.modes.get(usize::try_from(index).ok()?)?;
    if mode.infoType != DISPLAYCONFIG_MODE_INFO_TYPE_SOURCE {
        return None;
    }
    // SAFETY: `infoType` says the union holds a source mode.
    Some(unsafe { mode.Anonymous.sourceMode })
}

fn name_of(path: &DISPLAYCONFIG_PATH_INFO) -> Result<TargetName, SvcError> {
    ccd::target_name(path.targetInfo.adapterId, path.targetInfo.id)
}

/// Active displays with their names; MixLink monitors left out unless `with_mixlink`.
fn active(with_mixlink: bool) -> Result<Vec<SavedTarget>, SvcError> {
    let config = ccd::query(QDC_ONLY_ACTIVE_PATHS)?;
    let mut out = Vec::new();
    for path in &config.paths {
        let name = name_of(path)?;
        if !counts_as_display(&name, with_mixlink) {
            continue;
        }
        let Some(mode) = source_mode(&config, path) else {
            continue;
        };
        out.push(SavedTarget {
            monitor_path: name.monitor_path,
            x: mode.position.x,
            y: mode.position.y,
            width: mode.width,
            height: mode.height,
            refresh_num: path.targetInfo.refreshRate.Numerator,
            refresh_den: path.targetInfo.refreshRate.Denominator,
        });
    }
    Ok(out)
}

fn placement(target: &SavedTarget, x: i32, y: i32) -> Placement {
    Placement {
        monitor_path: target.monitor_path.clone(),
        x,
        y,
        width: target.width,
        height: target.height,
        refresh: DISPLAYCONFIG_RATIONAL {
            Numerator: target.refresh_num,
            Denominator: target.refresh_den,
        },
    }
}

/// Activates exactly `layout`: one path per display (active paths first, no source used twice),
/// our source modes with positions, target modes left to Windows (`SDC_ALLOW_CHANGES`).
fn apply_layout(layout: &[Placement], step: &'static str) -> Result<(), SvcError> {
    let all = ccd::query(QDC_ALL_PATHS)?;
    let mut ordered: Vec<&DISPLAYCONFIG_PATH_INFO> = all.paths.iter().collect();
    ordered.sort_by_key(|p| p.flags & DISPLAYCONFIG_PATH_ACTIVE == 0);
    let mut paths = Vec::with_capacity(layout.len());
    let mut modes = Vec::with_capacity(layout.len());
    for place in layout {
        let used = |p: &DISPLAYCONFIG_PATH_INFO| {
            paths.iter().any(|q: &DISPLAYCONFIG_PATH_INFO| {
                q.sourceInfo.adapterId == p.sourceInfo.adapterId
                    && q.sourceInfo.id == p.sourceInfo.id
            })
        };
        let mut chosen = None;
        for path in &ordered {
            if path.targetInfo.targetAvailable.as_bool()
                && !used(path)
                && name_of(path)?.monitor_path == place.monitor_path
            {
                chosen = Some(**path);
                break;
            }
        }
        let Some(mut path) = chosen else {
            continue;
        };
        let index = u32::try_from(modes.len()).unwrap_or(DISPLAYCONFIG_PATH_MODE_IDX_INVALID);
        modes.push(DISPLAYCONFIG_MODE_INFO {
            infoType: DISPLAYCONFIG_MODE_INFO_TYPE_SOURCE,
            id: path.sourceInfo.id,
            adapterId: path.sourceInfo.adapterId,
            Anonymous: DISPLAYCONFIG_MODE_INFO_0 {
                sourceMode: DISPLAYCONFIG_SOURCE_MODE {
                    width: place.width,
                    height: place.height,
                    pixelFormat: DISPLAYCONFIG_PIXELFORMAT_32BPP,
                    position: POINTL {
                        x: place.x,
                        y: place.y,
                    },
                },
            },
        });
        path.flags = DISPLAYCONFIG_PATH_ACTIVE;
        path.sourceInfo.Anonymous.modeInfoIdx = index;
        path.targetInfo.Anonymous.modeInfoIdx = DISPLAYCONFIG_PATH_MODE_IDX_INVALID;
        set_target_refresh(
            &mut path.targetInfo,
            place.refresh.Numerator,
            place.refresh.Denominator,
        );
        paths.push(path);
    }
    if paths.is_empty() {
        return Err(SvcError::Topology { step, code: 0 });
    }
    log_layout(step, &paths, &modes);
    ccd::apply(
        Some(&paths),
        Some(&modes),
        SDC_APPLY | SDC_USE_SUPPLIED_DISPLAY_CONFIG | SDC_SAVE_TO_DATABASE | SDC_ALLOW_CHANGES,
        step,
    )
}

impl DisplayConfig for CcdConfig {
    fn snapshot(&mut self) -> Result<Snapshot, SvcError> {
        Ok(Snapshot {
            version: SNAPSHOT_VERSION,
            targets: active(false)?,
        })
    }

    fn present_monitors(&mut self) -> Result<Vec<String>, SvcError> {
        let all = ccd::query(QDC_ALL_PATHS)?;
        let mut out: Vec<String> = Vec::new();
        for path in &all.paths {
            if path.targetInfo.targetAvailable.as_bool() {
                let name = name_of(path)?.monitor_path;
                if !out.contains(&name) {
                    out.push(name);
                }
            }
        }
        Ok(out)
    }

    fn find_mixlink(&mut self, product: u16) -> Result<Option<String>, SvcError> {
        let all = ccd::query(QDC_ALL_PATHS)?;
        for path in &all.paths {
            if !path.targetInfo.targetAvailable.as_bool() {
                continue;
            }
            let name = name_of(path)?;
            if is_ready_mixlink(&name, product) {
                log::debug!("find: MixLink monitor at {}", name.monitor_path);
                return Ok(Some(name.monitor_path));
            }
        }
        Ok(None)
    }

    fn has_physical_active(&mut self) -> Result<bool, SvcError> {
        Ok(!active(false)?.is_empty())
    }

    fn switch_to(
        &mut self,
        monitor_path: &str,
        mode: &DisplayInfo,
        topology: Topology,
        others: &[SavedTarget],
    ) -> Result<(), SvcError> {
        let mut layout = vec![Placement {
            monitor_path: monitor_path.to_owned(),
            x: 0,
            y: 0,
            width: mode.width,
            height: mode.height,
            refresh: DISPLAYCONFIG_RATIONAL {
                Numerator: mode.refresh_mhz,
                Denominator: 1000,
            },
        }];
        match topology {
            Topology::VirtualOnly => {}
            Topology::Extend => {
                for (target, placed) in others.iter().zip(plan_extend(mode.width, others)) {
                    layout.push(placement(target, placed.x, placed.y));
                }
            }
        }
        apply_layout(&layout, "switch")
    }

    fn restore(&mut self, plan: &RestorePlan) -> Result<(), SvcError> {
        match plan {
            RestorePlan::Apply(targets) => {
                let layout: Vec<Placement> =
                    targets.iter().map(|t| placement(t, t.x, t.y)).collect();
                apply_layout(&layout, "restore")
            }
            RestorePlan::ExtendFallback => ccd::apply(
                None,
                None,
                SDC_APPLY | SDC_TOPOLOGY_EXTEND,
                "restore (extend)",
            ),
        }
    }
}

/// `display probe`: can this process read and re-apply the current layout? (Session 0 check.)
pub fn probe_report() -> String {
    let mut out = String::new();
    match ccd::query(QDC_ONLY_ACTIVE_PATHS) {
        Ok(config) => {
            out.push_str(&format!("query: ok, {} active paths\n", config.paths.len()));
            for path in &config.paths {
                match name_of(path) {
                    Ok(name) => out.push_str(&format!(
                        "  mfg {:#06x} product {:#06x} {}\n",
                        name.edid_manufacture_id, name.edid_product_code_id, name.monitor_path
                    )),
                    Err(error) => out.push_str(&format!("  name: {error}\n")),
                }
            }
            let applied = ccd::apply(
                Some(&config.paths),
                Some(&config.modes),
                SDC_APPLY | SDC_USE_SUPPLIED_DISPLAY_CONFIG | SDC_ALLOW_CHANGES,
                "probe",
            );
            match applied {
                Ok(()) => out.push_str("apply: ok\n"),
                Err(error) => out.push_str(&format!("apply: {error}\n")),
            }
        }
        Err(error) => out.push_str(&format!("query: {error}\n")),
    }
    out
}

/// Refresh for a path whose target mode Windows picks (`SDC_ALLOW_CHANGES`). A path taken from an
/// inactive display reports scan-line ordering "unspecified"; with a refresh rate set,
/// SetDisplayConfig refuses that with ERROR_INVALID_PARAMETER, so it becomes progressive.
fn set_target_refresh(
    target: &mut DISPLAYCONFIG_PATH_TARGET_INFO,
    numerator: u32,
    denominator: u32,
) {
    target.refreshRate = DISPLAYCONFIG_RATIONAL {
        Numerator: numerator,
        Denominator: denominator,
    };
    if target.scanLineOrdering == DISPLAYCONFIG_SCANLINE_ORDERING_UNSPECIFIED {
        target.scanLineOrdering = DISPLAYCONFIG_SCANLINE_ORDERING_PROGRESSIVE;
    }
}

/// The paths and source modes `apply_layout` hands to SetDisplayConfig (`RUST_LOG=debug`).
fn log_layout(step: &str, paths: &[DISPLAYCONFIG_PATH_INFO], modes: &[DISPLAYCONFIG_MODE_INFO]) {
    if !log::log_enabled!(log::Level::Debug) {
        return;
    }
    for (path, mode) in paths.iter().zip(modes) {
        // SAFETY: `apply_layout` wrote these unions as a mode index and a source mode.
        let (target_mode, source) = unsafe {
            (
                path.targetInfo.Anonymous.modeInfoIdx,
                mode.Anonymous.sourceMode,
            )
        };
        let target = &path.targetInfo;
        log::debug!(
            "{step}: source {:#x}:{} target {:#x}:{} rot {} scale {} scan {} refresh {}/{} \
             status {:#x} target mode {target_mode:#x} source {}x{} at {},{}",
            path.sourceInfo.adapterId.LowPart,
            path.sourceInfo.id,
            target.adapterId.LowPart,
            target.id,
            target.rotation.0,
            target.scaling.0,
            target.scanLineOrdering.0,
            target.refreshRate.Numerator,
            target.refreshRate.Denominator,
            target.statusFlags,
            source.width,
            source.height,
            source.position.x,
            source.position.y,
        );
    }
    let active = ccd::query(QDC_ONLY_ACTIVE_PATHS).map_or(0, |c| c.paths.len());
    log::debug!("{step}: {active} active paths before apply");
}

#[cfg(test)]
#[path = "display_config_tests.rs"]
mod tests;
