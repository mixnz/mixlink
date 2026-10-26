use std::cell::RefCell;
use std::rc::Rc;

use super::*;
use crate::display::topology::{SNAPSHOT_VERSION, SavedTarget};

fn target(path: &str) -> SavedTarget {
    SavedTarget {
        monitor_path: path.to_owned(),
        x: 0,
        y: 0,
        width: 3840,
        height: 2160,
        refresh_num: 160,
        refresh_den: 1,
    }
}

const MODE: DisplayInfo = DisplayInfo {
    width: 2560,
    height: 1440,
    refresh_mhz: 120_000,
    hdr: false,
    dpi: 96,
};

/// Records calls; `fail` makes every layout change fail like SetDisplayConfig would.
#[derive(Default)]
struct Recorder {
    calls: Rc<RefCell<Vec<String>>>,
    fail: bool,
}

impl DisplayConfig for Recorder {
    fn snapshot(&mut self) -> Result<Snapshot, SvcError> {
        Ok(Snapshot {
            version: SNAPSHOT_VERSION,
            targets: vec![target("PHYS")],
        })
    }

    fn present_monitors(&mut self) -> Result<Vec<String>, SvcError> {
        Ok(vec!["PHYS".to_owned(), "VIRT".to_owned()])
    }

    fn find_mixlink(&mut self, product: u16) -> Result<Option<String>, SvcError> {
        Ok((product == 0x4D58).then(|| "VIRT".to_owned()))
    }

    fn has_physical_active(&mut self) -> Result<bool, SvcError> {
        Ok(true)
    }

    fn switch_to(
        &mut self,
        monitor_path: &str,
        mode: &DisplayInfo,
        topology: Topology,
        others: &[SavedTarget],
    ) -> Result<(), SvcError> {
        self.calls.borrow_mut().push(format!(
            "switch {monitor_path} {}x{} {topology:?} {}",
            mode.width,
            mode.height,
            others.len()
        ));
        if self.fail {
            return Err(SvcError::Topology {
                step: "switch",
                code: 87,
            });
        }
        Ok(())
    }

    fn restore(&mut self, plan: &RestorePlan) -> Result<(), SvcError> {
        self.calls.borrow_mut().push(format!("restore {plan:?}"));
        Ok(())
    }
}

/// Runs the helper side in-process, through JSON both ways, as the real helper does.
struct Loopback {
    helper: Recorder,
}

impl AgentRunner for Loopback {
    fn call(&mut self, request: &AgentRequest) -> Result<AgentReply, SvcError> {
        let text = serde_json::to_string(request).unwrap();
        let reply = handle(&mut self.helper, serde_json::from_str(&text).unwrap());
        let text = serde_json::to_string(&reply).unwrap();
        Ok(serde_json::from_str(&text).unwrap())
    }
}

fn client(fail: bool) -> (AgentConfig<Loopback>, Rc<RefCell<Vec<String>>>) {
    let calls = Rc::new(RefCell::new(Vec::new()));
    let helper = Recorder {
        calls: Rc::clone(&calls),
        fail,
    };
    (AgentConfig::new(Loopback { helper }), calls)
}

#[test]
fn queries_go_through_the_helper() {
    let (mut config, _) = client(false);
    assert_eq!(config.snapshot().unwrap().targets, vec![target("PHYS")]);
    assert_eq!(config.present_monitors().unwrap(), vec!["PHYS", "VIRT"]);
    assert_eq!(
        config.find_mixlink(0x4D58).unwrap(),
        Some("VIRT".to_owned())
    );
    assert_eq!(config.find_mixlink(0x4D59).unwrap(), None);
    assert!(config.has_physical_active().unwrap());
}

#[test]
fn changes_carry_all_arguments() {
    let (mut config, calls) = client(false);
    config
        .switch_to("VIRT", &MODE, Topology::Extend, &[target("PHYS")])
        .unwrap();
    config.restore(&RestorePlan::ExtendFallback).unwrap();
    assert_eq!(
        *calls.borrow(),
        vec!["switch VIRT 2560x1440 Extend 1", "restore ExtendFallback"]
    );
}

#[test]
fn helper_failure_becomes_an_error() {
    let (mut config, _) = client(true);
    let error = config
        .switch_to("VIRT", &MODE, Topology::VirtualOnly, &[])
        .unwrap_err();
    assert!(matches!(&error, SvcError::DisplayAgent(message) if message.contains("87")));
}

#[test]
fn wrong_reply_kind_is_an_error() {
    struct Wrong;
    impl AgentRunner for Wrong {
        fn call(&mut self, _: &AgentRequest) -> Result<AgentReply, SvcError> {
            Ok(AgentReply::Done)
        }
    }
    let mut config = AgentConfig::new(Wrong);
    assert!(matches!(config.snapshot(), Err(SvcError::DisplayAgent(_))));
}

#[test]
fn restore_plan_json_roundtrip() {
    let request = AgentRequest::Restore {
        plan: RestorePlan::Apply(vec![target(r"\\?\DISPLAY#MSI3CF0#5&361e361&0&UID4355")]),
    };
    let text = serde_json::to_string(&request).unwrap();
    assert_eq!(
        serde_json::from_str::<AgentRequest>(&text).unwrap(),
        request
    );
}
