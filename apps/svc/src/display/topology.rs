//! Screen layout snapshot and layout planning (spec 0015). Targets are keyed by monitor device path,
//! never by LUID (it changes per boot).

use serde::{Deserialize, Serialize};

pub const SNAPSHOT_VERSION: u32 = 1;

/// One active display as it was before the switch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedTarget {
    pub monitor_path: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub refresh_num: u32,
    pub refresh_den: u32,
}

/// The user's layout before MixLink changed it. The main display is the one at (0, 0).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    pub version: u32,
    pub targets: Vec<SavedTarget>,
}

impl Snapshot {
    pub fn empty() -> Snapshot {
        Snapshot {
            version: SNAPSHOT_VERSION,
            targets: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RestorePlan {
    /// The saved targets that are still present, in saved order.
    Apply(Vec<SavedTarget>),
    /// None of them is left: let Windows extend over what is there.
    ExtendFallback,
}

/// Where a display goes in the extend layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placed {
    pub monitor_path: String,
    pub x: i32,
    pub y: i32,
}

/// Saved targets whose monitor is still present (by device path).
pub fn plan_restore(saved: &Snapshot, present: &[String]) -> RestorePlan {
    let kept: Vec<SavedTarget> = saved
        .targets
        .iter()
        .filter(|t| present.contains(&t.monitor_path))
        .cloned()
        .collect();
    if kept.is_empty() {
        RestorePlan::ExtendFallback
    } else {
        RestorePlan::Apply(kept)
    }
}

/// The other displays keep their layout; their bounding box moves to the right of the virtual
/// monitor (left = `virtual_width`, top = 0).
pub fn plan_extend(virtual_width: u32, others: &[SavedTarget]) -> Vec<Placed> {
    let left = others.iter().map(|t| t.x).min().unwrap_or(0);
    let top = others.iter().map(|t| t.y).min().unwrap_or(0);
    let shift_x = i32::try_from(virtual_width).unwrap_or(i32::MAX) - left;
    others
        .iter()
        .map(|t| Placed {
            monitor_path: t.monitor_path.clone(),
            x: t.x + shift_x,
            y: t.y - top,
        })
        .collect()
}

#[cfg(test)]
#[path = "topology_tests.rs"]
mod tests;
