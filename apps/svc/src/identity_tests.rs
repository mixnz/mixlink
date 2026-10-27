use std::path::PathBuf;

use super::*;

fn temp_path(name: &str, text: Option<&str>) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mixlink-identity-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("identity.json");
    if let Some(text) = text {
        std::fs::write(&path, text).unwrap();
    }
    path
}

fn id(byte: u8) -> DeviceId {
    DeviceId::from_bytes([byte; 16])
}

#[test]
fn creates_when_missing() {
    let path = temp_path("missing", None);
    assert_eq!(load_or_create(&path, || Ok(id(1))).unwrap(), id(1));
    assert!(path.exists());
}

#[test]
fn creates_missing_folder() {
    let path = temp_path("nofolder", None)
        .with_file_name("sub")
        .join("identity.json");
    assert_eq!(load_or_create(&path, || Ok(id(4))).unwrap(), id(4));
    assert!(path.exists());
}

#[test]
fn keeps_existing_id() {
    let path = temp_path("keep", None);
    load_or_create(&path, || Ok(id(1))).unwrap();
    assert_eq!(load_or_create(&path, || Ok(id(2))).unwrap(), id(1));
}

#[test]
fn invalid_file_replaced() {
    let path = temp_path("invalid", Some("{"));
    assert_eq!(load_or_create(&path, || Ok(id(3))).unwrap(), id(3));
    assert_eq!(load_or_create(&path, || Ok(id(9))).unwrap(), id(3));
}

#[test]
fn wrong_length_id_replaced() {
    let path = temp_path("short", Some(r#"{"version":1,"id":"abcd"}"#));
    assert_eq!(load_or_create(&path, || Ok(id(5))).unwrap(), id(5));
}

#[test]
fn file_format() {
    let path = temp_path("format", None);
    load_or_create(&path, || Ok(id(0xab))).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    let value: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(value["version"], 1);
    assert_eq!(value["id"], "ab".repeat(16));
}
