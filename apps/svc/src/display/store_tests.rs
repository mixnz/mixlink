use std::path::PathBuf;

use super::*;
use crate::display::topology::{SNAPSHOT_VERSION, SavedTarget, Snapshot};

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mixlink-store-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn snapshot(x: i32) -> Snapshot {
    Snapshot {
        version: SNAPSHOT_VERSION,
        targets: vec![SavedTarget {
            monitor_path: "A".to_owned(),
            x,
            y: 0,
            width: 1920,
            height: 1080,
            refresh_num: 60_000,
            refresh_den: 1000,
        }],
    }
}

#[test]
fn save_if_absent_keeps_first() {
    let store = SnapshotStore::new(temp_dir("first").join("topology.json"));
    assert!(store.load().is_none());
    assert!(store.save_if_absent(&snapshot(1)).unwrap());
    assert!(!store.save_if_absent(&snapshot(2)).unwrap());
    assert_eq!(store.load(), Some(snapshot(1)));
}

#[test]
fn delete_then_load_none() {
    let store = SnapshotStore::new(temp_dir("delete").join("topology.json"));
    store.save_if_absent(&snapshot(1)).unwrap();
    store.delete().unwrap();
    assert!(store.load().is_none());
    store.delete().unwrap();
}

#[test]
fn load_corrupt_snapshot_returns_none_and_deletes() {
    let path = temp_dir("corrupt").join("topology.json");
    std::fs::write(&path, "{\"vers").unwrap();
    let store = SnapshotStore::new(path.clone());
    assert!(store.load().is_none());
    assert!(!path.exists());
}

#[test]
fn load_wrong_version_returns_none() {
    let path = temp_dir("version").join("topology.json");
    let mut old = snapshot(1);
    old.version = 99;
    std::fs::write(&path, serde_json::to_string(&old).unwrap()).unwrap();
    assert!(SnapshotStore::new(path.clone()).load().is_none());
    assert!(!path.exists());
}

#[test]
fn path_with_spaces() {
    let store = SnapshotStore::new(temp_dir("a b").join("sub dir").join("topology.json"));
    assert!(store.save_if_absent(&snapshot(3)).unwrap());
    assert_eq!(store.load(), Some(snapshot(3)));
}

#[test]
fn default_path_under_program_data() {
    let path = SnapshotStore::default_path(std::path::Path::new(r"C:\ProgramData"));
    assert_eq!(
        path,
        PathBuf::from(r"C:\ProgramData\MixLink\display\topology.json")
    );
}
