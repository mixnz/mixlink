use mixlink_input::host::abs_map::Rect;

use super::*;

fn target(path: &str, x: i32, y: i32) -> SavedTarget {
    SavedTarget {
        monitor_path: path.to_owned(),
        x,
        y,
        width: 1920,
        height: 1080,
        refresh_num: 60_000,
        refresh_den: 1000,
    }
}

fn snapshot(targets: Vec<SavedTarget>) -> Snapshot {
    Snapshot {
        version: SNAPSHOT_VERSION,
        targets,
    }
}

fn present(paths: &[&str]) -> Vec<String> {
    paths.iter().map(|p| (*p).to_owned()).collect()
}

#[test]
fn restore_plan_maps_by_path() {
    let saved = snapshot(vec![target("A", 0, 0), target("B", 1920, 0)]);
    assert_eq!(
        plan_restore(&saved, &present(&["B", "A", "C"])),
        RestorePlan::Apply(vec![target("A", 0, 0), target("B", 1920, 0)])
    );
}

#[test]
fn restore_plan_skips_missing() {
    let saved = snapshot(vec![target("A", 0, 0), target("B", 1920, 0)]);
    assert_eq!(
        plan_restore(&saved, &present(&["B"])),
        RestorePlan::Apply(vec![target("B", 1920, 0)])
    );
}

#[test]
fn restore_plan_falls_back_when_none_left() {
    let saved = snapshot(vec![target("A", 0, 0)]);
    assert_eq!(
        plan_restore(&saved, &present(&["C"])),
        RestorePlan::ExtendFallback
    );
    assert_eq!(
        plan_restore(&snapshot(Vec::new()), &present(&["C"])),
        RestorePlan::ExtendFallback
    );
}

#[test]
fn extend_moves_others_right() {
    let others = [target("A", 0, 0), target("B", 1920, 0)];
    assert_eq!(
        plan_extend(2560, &others),
        vec![
            Placed {
                monitor_path: "A".to_owned(),
                x: 2560,
                y: 0
            },
            Placed {
                monitor_path: "B".to_owned(),
                x: 4480,
                y: 0
            },
        ]
    );
}

#[test]
fn extend_keeps_vertical_offsets() {
    let others = [target("A", 0, 0), target("B", 0, -1080)];
    let placed = plan_extend(2560, &others);
    assert_eq!((placed[0].x, placed[0].y), (2560, 1080));
    assert_eq!((placed[1].x, placed[1].y), (2560, 0));
}

#[test]
fn snapshot_json_roundtrip() {
    let saved = snapshot(vec![target(r"\?\DISPLAY#DEL1234#5&abc", -1920, 0)]);
    let text = serde_json::to_string(&saved).unwrap();
    assert_eq!(serde_json::from_str::<Snapshot>(&text).unwrap(), saved);
}

#[test]
fn desktop_place_extend() {
    let mut physical = target("phys", 1920, 0);
    physical.width = 2560;
    physical.height = 1440;
    let snap = snapshot(vec![target("virt", 0, 0), physical]);
    let place = desktop_place(&snap, "virt").unwrap();
    assert_eq!(
        place.monitor,
        Rect {
            x: 0,
            y: 0,
            w: 1920,
            h: 1080
        }
    );
    assert_eq!(
        place.desktop,
        Rect {
            x: 0,
            y: 0,
            w: 4480,
            h: 1440
        }
    );
}

#[test]
fn desktop_place_negative_origin() {
    let mut physical = target("phys", -2560, -360);
    physical.width = 2560;
    physical.height = 1440;
    let snap = snapshot(vec![physical, target("virt", 0, 0)]);
    let place = desktop_place(&snap, "virt").unwrap();
    assert_eq!(
        place.desktop,
        Rect {
            x: -2560,
            y: -360,
            w: 4480,
            h: 1440
        }
    );
}

#[test]
fn desktop_place_missing_monitor() {
    let snap = snapshot(vec![target("phys", 0, 0)]);
    assert_eq!(desktop_place(&snap, "virt"), None);
}
