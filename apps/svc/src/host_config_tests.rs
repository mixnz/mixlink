use std::path::{Path, PathBuf};

use mixlink_core::config::Topology;

use super::*;

fn temp_file(name: &str, text: Option<&str>) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mixlink-hostcfg-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("host.json");
    if let Some(text) = text {
        std::fs::write(&path, text).unwrap();
    }
    path
}

#[test]
fn host_config_missing_is_default() {
    assert_eq!(load(&temp_file("missing", None)), HostConfig::default());
}

#[test]
fn host_config_invalid_is_default() {
    assert_eq!(
        load(&temp_file("invalid", Some("{\"display\":"))),
        HostConfig::default()
    );
    assert_eq!(
        load(&temp_file("badname", Some(r#"{"name":""}"#))),
        HostConfig::default()
    );
}

#[test]
fn host_config_extend() {
    let path = temp_file("extend", Some(r#"{"display":{"topology":"extend"}}"#));
    assert_eq!(load(&path).display.topology, Topology::Extend);
}

#[test]
fn host_config_default_path() {
    assert_eq!(
        default_path(Path::new(r"C:\ProgramData")),
        PathBuf::from(r"C:\ProgramData\MixLink\host.json")
    );
}
