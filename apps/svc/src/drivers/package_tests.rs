use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use super::*;

static NEXT: AtomicU32 = AtomicU32::new(0);

/// Fresh temp folder whose name contains a space (paths with spaces must work).
fn temp_root() -> PathBuf {
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("mixlink pkg {}-{n}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn touch_inf(dir: &Path, spec: &DriverSpec) {
    fs::create_dir_all(dir).unwrap();
    fs::write(dir.join(spec.inf), b"").unwrap();
}

fn vhid() -> &'static DriverSpec {
    DRIVERS.iter().find(|d| d.name == "vhid").unwrap()
}

#[test]
fn drivers_table() {
    let names: Vec<_> = DRIVERS.iter().map(|d| d.name).collect();
    assert_eq!(names, ["vdisplay", "vhid"]);
    assert_eq!(DRIVERS[0].hardware_ids, [r"Root\MixLinkVDisplay"]);
    // HID node first (created first, removed last), then the control node (spec 0016).
    assert_eq!(
        DRIVERS[1].hardware_ids,
        [r"Root\MixLinkVHid", r"Root\MixLinkVHidCtl"]
    );
    assert_eq!(DRIVERS[1].cat, "vhid.cat");
}

#[test]
fn resolve_default_dist() {
    let exe = temp_root();
    let dir = exe.join("drivers").join("vhid");
    touch_inf(&dir, vhid());
    assert_eq!(resolve_source(&exe, None, vhid()).unwrap(), dir);
}

#[test]
fn resolve_from_prefers_pkg() {
    let from = temp_root();
    touch_inf(&from.join("vhid").join("pkg"), vhid());
    touch_inf(&from.join("vhid"), vhid());
    let got = resolve_source(Path::new("unused"), Some(&from), vhid()).unwrap();
    assert_eq!(got, from.join("vhid").join("pkg"));
}

#[test]
fn resolve_from_without_pkg_level() {
    let from = temp_root();
    touch_inf(&from.join("vhid"), vhid());
    let got = resolve_source(Path::new("unused"), Some(&from), vhid()).unwrap();
    assert_eq!(got, from.join("vhid"));
}

#[test]
fn resolve_from_with_spaces() {
    let from = temp_root().join("My Builds");
    touch_inf(&from.join("vhid").join("pkg"), vhid());
    let got = resolve_source(Path::new("unused"), Some(&from), vhid()).unwrap();
    assert_eq!(got, from.join("vhid").join("pkg"));
}

#[test]
fn resolve_missing_names_path() {
    let exe = temp_root();
    let err = resolve_source(&exe, None, vhid()).unwrap_err();
    let expected = exe.join("drivers").join("vhid");
    match &err {
        SvcError::PackageMissing { path } => assert_eq!(path, &expected),
        other => panic!("unexpected error: {other:?}"),
    }
    let text = err.to_string();
    assert!(text.contains(&expected.display().to_string()), "{text}");
    assert!(text.contains("--from"), "{text}");
}

#[test]
fn stage_dir_layout() {
    assert_eq!(
        stage_dir(Path::new(r"C:\ProgramData"), "vhid"),
        PathBuf::from(r"C:\ProgramData\MixLink\drivers\vhid")
    );
}
