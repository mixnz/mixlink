//! One client session on the virtual display (spec 0015): add the monitor, switch the layout, put it back.
//! Also the service start: restore a layout a crash left switched, add the headless monitor.

use std::time::Duration;

use mixlink_core::config::Topology;
use mixlink_core::ids::DeviceId;
use mixlink_proto::control::DisplayInfo;

use super::edid::{HEADLESS_ID, HEADLESS_MODE, PRODUCT_CLIENT, PRODUCT_HEADLESS};
use super::modes::add_request;
use super::ports::{DisplayConfig, DisplayDriver};
use super::store::SnapshotStore;
use super::topology::{SavedTarget, Snapshot, plan_restore};
use crate::error::SvcError;

/// How often and how many times `begin` looks for the new monitor in Windows' list (≤ 5 s).
const TARGET_POLL: Duration = Duration::from_millis(100);
const TARGET_POLLS: u32 = 50;

/// What a session works on.
#[derive(Debug)]
pub struct Ports<'a, D: DisplayDriver, C: DisplayConfig> {
    pub driver: &'a mut D,
    pub config: &'a mut C,
    pub store: &'a SnapshotStore,
}

/// A client monitor that is up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Active {
    pub monitor: u32,
    pub client: DeviceId,
}

/// Adds the client's monitor and, unless `topology` is `None` (keep the layout), saves the layout
/// and makes the monitor the main display, then removes the headless monitor. On failure the
/// steps already done are undone.
pub fn begin<D: DisplayDriver, C: DisplayConfig>(
    p: &mut Ports<'_, D, C>,
    client: &DeviceId,
    mode: &DisplayInfo,
    topology: Option<Topology>,
    headless: &mut Option<u32>,
    sleep: &dyn Fn(Duration),
) -> Result<Active, SvcError> {
    // The layout before the monitor arrives: Windows may switch to a layout it remembers for this
    // client's monitor the moment it appears, so `extend` must not read the displays after that.
    let before = match topology {
        Some(_) => p.config.snapshot()?,
        None => Snapshot::empty(),
    };
    let saved = topology.is_some() && p.store.save_if_absent(&before)?;
    let monitor = match p
        .driver
        .add_monitor(&add_request(client, PRODUCT_CLIENT, mode))
    {
        Ok(monitor) => monitor,
        Err(error) => {
            undo(p, None, saved);
            return Err(error);
        }
    };
    if let Some(topology) = topology {
        if let Err(error) = switch(p, mode, topology, &before.targets, sleep) {
            undo(p, Some(monitor), saved);
            return Err(error);
        }
        if let Some(id) = headless.take()
            && let Err(error) = p.driver.remove_monitor(id)
        {
            log::warn!("cannot remove the headless monitor: {error}");
        }
    }
    Ok(Active {
        monitor,
        client: *client,
    })
}

fn switch<D: DisplayDriver, C: DisplayConfig>(
    p: &mut Ports<'_, D, C>,
    mode: &DisplayInfo,
    topology: Topology,
    others: &[SavedTarget],
    sleep: &dyn Fn(Duration),
) -> Result<(), SvcError> {
    for poll in 0..TARGET_POLLS {
        if poll > 0 {
            sleep(TARGET_POLL);
        }
        if let Some(path) = p.config.find_mixlink(PRODUCT_CLIENT)? {
            return p.config.switch_to(&path, mode, topology, others);
        }
    }
    Err(SvcError::MonitorTimeout)
}

fn undo<D: DisplayDriver, C: DisplayConfig>(
    p: &mut Ports<'_, D, C>,
    monitor: Option<u32>,
    saved: bool,
) {
    if let Some(id) = monitor
        && let Err(error) = p.driver.remove_monitor(id)
    {
        log::warn!("cannot remove monitor {id}: {error}");
    }
    if saved && let Err(error) = restore_saved(p) {
        log::warn!("cannot put the screen layout back: {error}");
    }
}

/// Applies the saved layout (if any) and deletes it once applied.
fn restore_saved<D: DisplayDriver, C: DisplayConfig>(
    p: &mut Ports<'_, D, C>,
) -> Result<(), SvcError> {
    let Some(snapshot) = p.store.load() else {
        return Ok(());
    };
    let present = p.config.present_monitors()?;
    p.config.restore(&plan_restore(&snapshot, &present))?;
    p.store.delete()
}

fn add_headless_if_needed<D: DisplayDriver, C: DisplayConfig>(
    p: &mut Ports<'_, D, C>,
    headless: &mut Option<u32>,
) -> Result<(), SvcError> {
    if headless.is_none() && !p.config.has_physical_active()? {
        *headless = Some(p.driver.add_monitor(&add_request(
            &HEADLESS_ID,
            PRODUCT_HEADLESS,
            &HEADLESS_MODE,
        ))?);
    }
    Ok(())
}

/// Removes the client's monitor, puts the saved layout back, adds the headless monitor when no
/// physical monitor is active.
pub fn end<D: DisplayDriver, C: DisplayConfig>(
    p: &mut Ports<'_, D, C>,
    active: Active,
    headless: &mut Option<u32>,
) -> Result<(), SvcError> {
    p.driver.remove_monitor(active.monitor)?;
    restore_saved(p)?;
    add_headless_if_needed(p, headless)
}

/// Service start: restore a layout a crash or power loss left switched, then add the headless
/// monitor when no physical monitor is active (login screen streamable, ADR-0006).
pub fn start_up<D: DisplayDriver, C: DisplayConfig>(
    p: &mut Ports<'_, D, C>,
    headless: &mut Option<u32>,
) -> Result<(), SvcError> {
    restore_saved(p)?;
    add_headless_if_needed(p, headless)
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;
