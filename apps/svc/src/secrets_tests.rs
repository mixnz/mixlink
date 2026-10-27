use mixlink_core::ids::DeviceId;
use mixlink_proto::control::PublicKey;

use super::*;

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mixlink-secrets-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn device(byte: u8) -> PairedDevice {
    PairedDevice {
        device: DeviceId::from_bytes([byte; 16]),
        name: "Laptop".to_owned(),
        key: PublicKey([byte; 32]),
        paired_at: 1_700_000_000,
    }
}

#[test]
fn key_created_once() {
    let dir = temp_dir("key");
    let first = load_or_create_key(&dir, || Ok(StaticKeyPair::generate().unwrap())).unwrap();
    let second = load_or_create_key(&dir, || panic!("must load the saved key")).unwrap();
    assert_eq!(first, second);
}

#[test]
fn corrupt_key_file_makes_new_key() {
    let dir = temp_dir("badkey");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(key_path(&dir), "{oops").unwrap();
    let key = StaticKeyPair::generate().unwrap();
    let expected = key.clone();
    assert_eq!(load_or_create_key(&dir, || Ok(key)).unwrap(), expected);
}

#[test]
fn missing_paired_file_gives_empty_list() {
    assert!(load_paired(&temp_dir("nopaired")).is_empty());
}

#[test]
fn corrupt_paired_file_gives_empty_list() {
    let dir = temp_dir("badpaired");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(paired_path(&dir), "[1, 2").unwrap();
    assert!(load_paired(&dir).is_empty());
}

#[test]
fn paired_round_trips() {
    let dir = temp_dir("paired");
    let devices = vec![device(1), device(2)];
    save_paired(&dir, &devices).unwrap();
    assert_eq!(load_paired(&dir), devices);
}
