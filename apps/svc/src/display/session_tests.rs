use mixlink_core::config::Topology;

use super::*;
use crate::display::fakes::*;

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
