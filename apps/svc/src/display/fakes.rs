//! Test doubles for the display ports (spec 0015), shared by the session and request-loop tests.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use mixlink_core::config::Topology;
use mixlink_core::ids::DeviceId;
use mixlink_proto::control::DisplayInfo;

use super::ports::{DisplayConfig, DisplayDriver};
use super::session::Ports;
use super::store::SnapshotStore;
use super::topology::{RestorePlan, SNAPSHOT_VERSION, SavedTarget, Snapshot};
use super::wire::{AddMonitor, MonitorInfo, QueryReply, WireMode};
use crate::error::SvcError;

pub(crate) type Log = Rc<RefCell<Vec<String>>>;

pub(crate) struct FakeDriver {
    pub(crate) log: Log,
    pub(crate) next_id: u32,
    /// Monitors up now; connector = id - 1.
    pub(crate) monitors: Vec<u32>,
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
        self.monitors.push(id);
        Ok(id)
    }

    fn remove_monitor(&mut self, id: u32) -> Result<(), SvcError> {
        self.log.borrow_mut().push(format!("remove {id}"));
        self.monitors.retain(|&m| m != id);
        Ok(())
    }

    fn query(&mut self) -> Result<QueryReply, SvcError> {
        Ok(QueryReply {
            iddcx_version: 0,
            render_luid: (0, 0),
            monitors: self
                .monitors
                .iter()
                .map(|&id| MonitorInfo {
                    id,
                    connector: id - 1,
                    mode: WireMode {
                        width: 0,
                        height: 0,
                        refresh_mhz: 0,
                    },
                })
                .collect(),
        })
    }
}

pub(crate) struct FakeConfig {
    pub(crate) log: Log,
    /// `find_mixlink` answers after this many polls; `None` = never.
    pub(crate) found_after: Option<u32>,
    pub(crate) polls: Cell<u32>,
    pub(crate) fail_switch: bool,
    pub(crate) physical: bool,
    pub(crate) snapshots_taken: Cell<u32>,
}

pub(crate) fn saved() -> Snapshot {
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

pub(crate) struct Rig {
    pub(crate) log: Log,
    pub(crate) driver: FakeDriver,
    pub(crate) config: FakeConfig,
    pub(crate) store: SnapshotStore,
}

pub(crate) fn rig(name: &str) -> Rig {
    let dir: PathBuf =
        std::env::temp_dir().join(format!("mixlink-session-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let log: Log = Rc::default();
    Rig {
        driver: FakeDriver {
            log: Rc::clone(&log),
            next_id: 1,
            monitors: Vec::new(),
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

pub(crate) fn client() -> DeviceId {
    DeviceId::from_bytes([7; 16])
}

pub(crate) const MODE: DisplayInfo = DisplayInfo {
    width: 2560,
    height: 1440,
    refresh_mhz: 120_000,
    hdr: false,
    dpi: 96,
};

pub(crate) fn ports(r: &mut Rig) -> Ports<'_, FakeDriver, FakeConfig> {
    Ports {
        driver: &mut r.driver,
        config: &mut r.config,
        store: &r.store,
    }
}

pub(crate) fn no_sleep(_: Duration) {}
