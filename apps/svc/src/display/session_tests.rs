use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

use mixlink_core::config::Topology;

use super::*;
use crate::display::topology::{RestorePlan, SNAPSHOT_VERSION, SavedTarget, Snapshot};
use crate::display::wire::{AddMonitor, QueryReply};

type Log = Rc<RefCell<Vec<String>>>;

struct FakeDriver {
    log: Log,
    next_id: u32,
}

impl DisplayDriver for FakeDriver {
    fn add_monitor(&mut self, req: &AddMonitor) -> Result<u32, SvcError> {
        let id = self.next_id;
        self.next_id += 1;
        // Product code: EDID bytes 10..12.
        let product = u16::from_le_bytes([req.edid[10], req.edid[11]]);
        self.log
            .borrow_mut()
            .push(format!("add {product:#x} -> {id}"));
        Ok(id)
    }

    fn remove_monitor(&mut self, id: u32) -> Result<(), SvcError> {
        self.log.borrow_mut().push(format!("remove {id}"));
        Ok(())
    }

    fn query(&mut self) -> Result<QueryReply, SvcError> {
        Ok(QueryReply {
            iddcx_version: 0,
            render_luid: (0, 0),
            monitors: Vec::new(),
        })
    }
}

struct FakeConfig {
    log: Log,
    /// `find_mixlink` answers after this many polls; `None` = never.
    found_after: Option<u32>,
    polls: Cell<u32>,
    fail_switch: bool,
    physical: bool,
    snapshots_taken: Cell<u32>,
}

fn saved() -> Snapshot {
    Snapshot {
        version: SNAPSHOT_VERSION,
        targets: vec![SavedTarget {
            monitor_path: "PHYS".to_owned(),
            x: 0,
            y: 0,
            width: 2560,
            height: 1440,
            refresh_num: 144_000,
            refresh_den: 1000,
        }],
    }
}

impl DisplayConfig for FakeConfig {
    fn snapshot(&mut self) -> Result<Snapshot, SvcError> {
        self.log.borrow_mut().push("snapshot".to_owned());
        // After the first look, Windows has applied the layout it remembers for this monitor set
        // (the physical display off), as it does when a known client monitor arrives.
        let first = self.snapshots_taken.get() == 0;
        self.snapshots_taken.set(self.snapshots_taken.get() + 1);
        let mut snapshot = saved();
        if !first {
            snapshot.targets.clear();
        }
        Ok(snapshot)
    }

    fn present_monitors(&mut self) -> Result<Vec<String>, SvcError> {
        Ok(vec!["PHYS".to_owned()])
    }

    fn find_mixlink(&mut self, product: u16) -> Result<Option<String>, SvcError> {
        let polls = self.polls.get() + 1;
        self.polls.set(polls);
        Ok(match self.found_after {
            Some(after) if polls >= after => Some(format!("VIRT{product:#x}")),
            _ => None,
        })
    }

    fn has_physical_active(&mut self) -> Result<bool, SvcError> {
        Ok(self.physical)
    }

    fn switch_to(
        &mut self,
        monitor_path: &str,
        _mode: &DisplayInfo,
        topology: Topology,
        others: &[SavedTarget],
    ) -> Result<(), SvcError> {
        let others: Vec<&str> = others.iter().map(|t| t.monitor_path.as_str()).collect();
        self.log.borrow_mut().push(format!(
            "switch {monitor_path} {topology:?} [{}]",
            others.join(",")
        ));
        if self.fail_switch {
            return Err(SvcError::Topology {
                step: "switch",
                code: 87,
            });
        }
        Ok(())
    }

    fn restore(&mut self, plan: &RestorePlan) -> Result<(), SvcError> {
        let text = match plan {
            RestorePlan::Apply(targets) => format!("restore {}", targets.len()),
            RestorePlan::ExtendFallback => "restore extend".to_owned(),
        };
        self.log.borrow_mut().push(text);
        Ok(())
    }
}

struct Rig {
    log: Log,
    driver: FakeDriver,
    config: FakeConfig,
    store: SnapshotStore,
}

fn rig(name: &str) -> Rig {
    let dir: PathBuf =
        std::env::temp_dir().join(format!("mixlink-session-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let log: Log = Rc::default();
    Rig {
        driver: FakeDriver {
            log: Rc::clone(&log),
            next_id: 1,
        },
        config: FakeConfig {
            log: Rc::clone(&log),
            found_after: Some(1),
            polls: Cell::new(0),
            fail_switch: false,
            physical: true,
            snapshots_taken: Cell::new(0),
        },
        store: SnapshotStore::new(dir.join("topology.json")),
        log,
    }
}

fn client() -> DeviceId {
    DeviceId::from_bytes([7; 16])
}

const MODE: DisplayInfo = DisplayInfo {
    width: 2560,
    height: 1440,
    refresh_mhz: 120_000,
    hdr: false,
    dpi: 96,
};

fn ports(r: &mut Rig) -> Ports<'_, FakeDriver, FakeConfig> {
    Ports {
        driver: &mut r.driver,
        config: &mut r.config,
        store: &r.store,
    }
}

fn no_sleep(_: Duration) {}

fn begin_with(
    r: &mut Rig,
    topology: Option<Topology>,
    headless: &mut Option<u32>,
) -> Result<Active, SvcError> {
    begin(
        &mut ports(r),
        &client(),
        &MODE,
        topology,
        headless,
        &no_sleep,
    )
}

fn entries(r: &Rig) -> Vec<String> {
    r.log.borrow().clone()
}

#[test]
fn begin_saves_adds_switches_in_order() {
    let mut r = rig("order");
    let active = begin_with(&mut r, Some(Topology::VirtualOnly), &mut None).unwrap();
    assert_eq!(active.monitor, 1);
    assert_eq!(
        entries(&r),
        vec![
            "snapshot",
            "add 0x4d58 -> 1",
            "switch VIRT0x4d58 VirtualOnly [PHYS]"
        ]
    );
    assert_eq!(r.store.load(), Some(saved()));
}

#[test]
fn begin_keep_skips_save_and_switch() {
    let mut r = rig("keep");
    let mut headless = Some(9);
    begin_with(&mut r, None, &mut headless).unwrap();
    assert_eq!(entries(&r), vec!["add 0x4d58 -> 1"]);
    assert_eq!(headless, Some(9));
    assert!(r.store.load().is_none());
}

#[test]
fn begin_removes_headless() {
    let mut r = rig("headless");
    let mut headless = Some(9);
    begin_with(&mut r, Some(Topology::Extend), &mut headless).unwrap();
    assert_eq!(entries(&r).last().map(String::as_str), Some("remove 9"));
    assert_eq!(headless, None);
}

#[test]
fn begin_failure_undoes_steps() {
    let mut r = rig("undo");
    r.config.fail_switch = true;
    let result = begin_with(&mut r, Some(Topology::VirtualOnly), &mut None);
    assert!(matches!(result, Err(SvcError::Topology { .. })));
    assert_eq!(
        entries(&r),
        vec![
            "snapshot",
            "add 0x4d58 -> 1",
            "switch VIRT0x4d58 VirtualOnly [PHYS]",
            "remove 1",
            "restore 1"
        ]
    );
    assert!(r.store.load().is_none());
}

#[test]
fn begin_timeout_when_target_never_appears() {
    let mut r = rig("timeout");
    r.config.found_after = None;
    let result = begin_with(&mut r, Some(Topology::VirtualOnly), &mut None);
    assert!(matches!(result, Err(SvcError::MonitorTimeout)));
    assert_eq!(r.config.polls.get(), 50);
    assert!(entries(&r).contains(&"remove 1".to_owned()));
}

#[test]
fn begin_waits_for_target() {
    let mut r = rig("wait");
    r.config.found_after = Some(4);
    begin_with(&mut r, Some(Topology::VirtualOnly), &mut None).unwrap();
    assert_eq!(r.config.polls.get(), 4);
}

#[test]
fn existing_snapshot_not_overwritten() {
    let mut r = rig("existing");
    let mut older = saved();
    older.targets[0].x = -1920;
    r.store.save_if_absent(&older).unwrap();
    let active = begin_with(&mut r, Some(Topology::VirtualOnly), &mut None).unwrap();
    assert_eq!(r.store.load(), Some(older));
    let mut headless = None;
    end(&mut ports(&mut r), active, &mut headless).unwrap();
    assert!(r.store.load().is_none());
}

#[test]
fn end_restores_and_deletes() {
    let mut r = rig("end");
    let active = begin_with(&mut r, Some(Topology::VirtualOnly), &mut None).unwrap();
    r.log.borrow_mut().clear();
    let mut headless = None;
    end(&mut ports(&mut r), active, &mut headless).unwrap();
    assert_eq!(entries(&r), vec!["remove 1", "restore 1"]);
    assert!(r.store.load().is_none());
    assert_eq!(headless, None);
}

#[test]
fn end_adds_headless_without_physical() {
    let mut r = rig("end-headless");
    let active = begin_with(&mut r, Some(Topology::VirtualOnly), &mut None).unwrap();
    r.config.physical = false;
    let mut headless = None;
    end(&mut ports(&mut r), active, &mut headless).unwrap();
    assert_eq!(
        entries(&r).last().map(String::as_str),
        Some("add 0x4d59 -> 2")
    );
    assert_eq!(headless, Some(2));
}

#[test]
fn start_up_restores_leftover_snapshot() {
    let mut r = rig("startup");
    r.store.save_if_absent(&saved()).unwrap();
    let mut headless = None;
    start_up(&mut ports(&mut r), &mut headless).unwrap();
    assert_eq!(entries(&r), vec!["restore 1"]);
    assert!(r.store.load().is_none());
    assert_eq!(headless, None);
}

#[test]
fn start_up_no_headless_with_physical() {
    let mut r = rig("startup-phys");
    let mut headless = None;
    start_up(&mut ports(&mut r), &mut headless).unwrap();
    assert!(entries(&r).is_empty());
    assert_eq!(headless, None);
}

#[test]
fn start_up_adds_headless_without_physical() {
    let mut r = rig("startup-headless");
    r.config.physical = false;
    let mut headless = None;
    start_up(&mut ports(&mut r), &mut headless).unwrap();
    assert_eq!(entries(&r), vec!["add 0x4d59 -> 1"]);
    assert_eq!(headless, Some(1));
}

#[test]
fn begin_extend_keeps_displays_from_before_the_monitor() {
    let mut r = rig("extend-before");
    begin_with(&mut r, Some(Topology::Extend), &mut None).unwrap();
    assert!(entries(&r).contains(&"switch VIRT0x4d58 Extend [PHYS]".to_owned()));
    assert_eq!(r.config.snapshots_taken.get(), 1);
}
