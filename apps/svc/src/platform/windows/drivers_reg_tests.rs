use std::time::{SystemTime, UNIX_EPOCH};

use windows::Win32::System::Registry::HKEY_CURRENT_USER;

use super::*;

/// A per-test key under HKCU, deleted on drop.
struct TestKey(String);

impl TestKey {
    fn new() -> TestKey {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        TestKey(format!(
            r"Software\MixLinkTest\{}-{nanos}",
            std::process::id()
        ))
    }
}

impl Drop for TestKey {
    fn drop(&mut self) {
        let _ = delete_state(HKEY_CURRENT_USER, &self.0);
    }
}

fn sample() -> DriverState {
    let mut state = DriverState {
        cert_thumbprint: Some("0123456789ABCDEF0123456789ABCDEF01234567".to_owned()),
        ..DriverState::default()
    };
    state
        .published
        .insert("vdisplay".to_owned(), "oem10.inf".to_owned());
    state
        .published
        .insert("vhid".to_owned(), "oem11.inf".to_owned());
    state
}

#[test]
fn missing_key_is_empty_state() {
    let key = TestKey::new();
    assert_eq!(
        load_state(HKEY_CURRENT_USER, &key.0).unwrap(),
        DriverState::default()
    );
}

#[test]
fn save_then_load_round_trip() {
    let key = TestKey::new();
    save_state(HKEY_CURRENT_USER, &key.0, &sample()).unwrap();
    assert_eq!(load_state(HKEY_CURRENT_USER, &key.0).unwrap(), sample());
}

#[test]
fn save_replaces_old_values() {
    let key = TestKey::new();
    save_state(HKEY_CURRENT_USER, &key.0, &sample()).unwrap();
    let mut smaller = sample();
    smaller.published.remove("vhid");
    save_state(HKEY_CURRENT_USER, &key.0, &smaller).unwrap();
    assert_eq!(load_state(HKEY_CURRENT_USER, &key.0).unwrap(), smaller);
}

#[test]
fn delete_then_load_is_empty() {
    let key = TestKey::new();
    save_state(HKEY_CURRENT_USER, &key.0, &sample()).unwrap();
    delete_state(HKEY_CURRENT_USER, &key.0).unwrap();
    assert!(load_state(HKEY_CURRENT_USER, &key.0).unwrap().is_empty());
}

#[test]
fn delete_missing_ok() {
    let key = TestKey::new();
    delete_state(HKEY_CURRENT_USER, &key.0).unwrap();
}

#[test]
fn load_rejects_bad_value() {
    let key = TestKey::new();
    write_values(
        HKEY_CURRENT_USER,
        &key.0,
        &[("vhid.PublishedInf".to_owned(), "x".to_owned())],
    )
    .unwrap();
    assert!(matches!(
        load_state(HKEY_CURRENT_USER, &key.0),
        Err(SvcError::StateInvalid(_))
    ));
}
