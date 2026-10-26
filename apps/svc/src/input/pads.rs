//! Virtual Xbox pad nodes per slot (spec 0016): add (replacing a node left by a crash), remove,
//! remove all. Pure over `PadHost`; `platform::windows::pad_nodes` is the real host.

use std::time::Duration;

use super::pad_ids::{PAD_SLOTS, valid_slot};
use crate::drivers::flow::RemoveOutcome;
use crate::error::SvcError;

/// How long `add` waits for the new node to register with the driver.
pub const READY_TIMEOUT: Duration = Duration::from_secs(5);
const READY_POLL: Duration = Duration::from_millis(100);

pub trait PadHost {
    fn exists(&mut self, slot: u8) -> Result<bool, SvcError>;
    fn create(&mut self, slot: u8) -> Result<(), SvcError>;
    fn remove(&mut self, slot: u8) -> Result<RemoveOutcome, SvcError>;
    /// Bit per slot whose node the driver feeds (`QUERY` pads present).
    fn present(&mut self) -> Result<u8, SvcError>;
    fn sleep(&mut self, duration: Duration);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddOutcome {
    Added,
    /// A node was already there (left by a crash) and was recreated.
    Replaced,
}

/// Creates the pad node of `slot` and waits until the driver feeds it.
pub fn add(host: &mut impl PadHost, slot: u8) -> Result<AddOutcome, SvcError> {
    if !valid_slot(slot) {
        return Err(SvcError::Usage(format!("pad slot {slot} is out of range")));
    }
    let outcome = if host.exists(slot)? {
        held_to_error(host.remove(slot)?)?;
        AddOutcome::Replaced
    } else {
        AddOutcome::Added
    };
    host.create(slot)?;
    let mut waited = Duration::ZERO;
    while host.present()? & (1 << slot) == 0 {
        if waited >= READY_TIMEOUT {
            return Err(SvcError::PadTimeout(slot));
        }
        host.sleep(READY_POLL);
        waited += READY_POLL;
    }
    Ok(outcome)
}

pub fn remove(host: &mut impl PadHost, slot: u8) -> Result<RemoveOutcome, SvcError> {
    if !valid_slot(slot) {
        return Err(SvcError::Usage(format!("pad slot {slot} is out of range")));
    }
    host.remove(slot)
}

/// Removes every pad node (service start after a crash, `drivers uninstall`).
pub fn remove_all(host: &mut impl PadHost) -> Result<(), SvcError> {
    for slot in 0..PAD_SLOTS {
        held_to_error(host.remove(slot)?)?;
    }
    Ok(())
}

fn held_to_error(outcome: RemoveOutcome) -> Result<(), SvcError> {
    match outcome {
        RemoveOutcome::Held { app } => Err(SvcError::DeviceHeld(app)),
        RemoveOutcome::Removed | RemoveOutcome::NoDevice => Ok(()),
    }
}

#[cfg(test)]
#[path = "pads_tests.rs"]
mod tests;
