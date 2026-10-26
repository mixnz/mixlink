use std::time::Duration;

use super::*;
use crate::drivers::flow::RemoveOutcome;
use crate::error::SvcError;

/// Records calls; `nodes` is the set of slots with a node, `ready` the slots the driver reports.
#[derive(Default)]
struct FakeHost {
    nodes: u8,
    ready_after_create: bool,
    held: bool,
    calls: Vec<String>,
    slept: Duration,
}

impl PadHost for FakeHost {
    fn exists(&mut self, slot: u8) -> Result<bool, SvcError> {
        Ok(self.nodes & (1 << slot) != 0)
    }

    fn create(&mut self, slot: u8) -> Result<(), SvcError> {
        self.calls.push(format!("create {slot}"));
        self.nodes |= 1 << slot;
        Ok(())
    }

    fn remove(&mut self, slot: u8) -> Result<RemoveOutcome, SvcError> {
        self.calls.push(format!("remove {slot}"));
        if self.held {
            return Ok(RemoveOutcome::Held {
                app: Some("Game".into()),
            });
        }
        if self.nodes & (1 << slot) == 0 {
            return Ok(RemoveOutcome::NoDevice);
        }
        self.nodes &= !(1 << slot);
        Ok(RemoveOutcome::Removed)
    }

    fn present(&mut self) -> Result<u8, SvcError> {
        Ok(if self.ready_after_create {
            self.nodes
        } else {
            0
        })
    }

    fn sleep(&mut self, duration: Duration) {
        self.slept += duration;
    }
}

fn host() -> FakeHost {
    FakeHost {
        ready_after_create: true,
        ..FakeHost::default()
    }
}

#[test]
fn add_creates_and_waits_for_the_driver() {
    let mut h = host();
    assert_eq!(add(&mut h, 1).unwrap(), AddOutcome::Added);
    assert_eq!(h.calls, ["create 1"]);
    assert_eq!(h.nodes, 0b10);
}

#[test]
fn plan_add_replaces_leftover() {
    let mut h = FakeHost {
        nodes: 0b1,
        ..host()
    };
    assert_eq!(add(&mut h, 0).unwrap(), AddOutcome::Replaced);
    assert_eq!(h.calls, ["remove 0", "create 0"]);
    assert_eq!(h.nodes, 0b1);
}

#[test]
fn add_rejects_slot_4() {
    let mut h = host();
    assert!(matches!(add(&mut h, 4), Err(SvcError::Usage(_))));
    assert!(h.calls.is_empty());
}

#[test]
fn add_times_out_when_the_pad_never_registers() {
    let mut h = FakeHost::default();
    assert!(matches!(add(&mut h, 2), Err(SvcError::PadTimeout(2))));
    assert!(h.slept >= READY_TIMEOUT);
}

#[test]
fn remove_absent_is_ok() {
    let mut h = host();
    assert!(matches!(
        remove(&mut h, 3).unwrap(),
        RemoveOutcome::NoDevice
    ));
}

#[test]
fn remove_all_walks_every_slot() {
    let mut h = FakeHost {
        nodes: 0b1010,
        ..host()
    };
    remove_all(&mut h).unwrap();
    assert_eq!(h.calls, ["remove 0", "remove 1", "remove 2", "remove 3"]);
    assert_eq!(h.nodes, 0);
}

#[test]
fn remove_all_stops_when_held() {
    let mut h = FakeHost {
        nodes: 0b1,
        held: true,
        ..host()
    };
    assert!(matches!(
        remove_all(&mut h),
        Err(SvcError::DeviceHeld(Some(_)))
    ));
}
