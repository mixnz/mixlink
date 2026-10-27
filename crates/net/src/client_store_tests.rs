use mixlink_proto::control::PublicKey;

use super::*;

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mixlink-client-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn host(byte: u8) -> KnownHost {
    KnownHost {
        host: DeviceId::from_bytes([byte; 16]),
        name: format!("Host {byte}"),
        key: PublicKey([byte; 32]),
    }
}

#[test]
fn identity_created_once() {
    let dir = temp_dir("identity");
    let first = load_or_create_identity(&dir).unwrap();
    let second = load_or_create_identity(&dir).unwrap();
    assert_eq!(first, second);
}

#[test]
fn corrupt_identity_replaced() {
    let dir = temp_dir("badidentity");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(identity_path(&dir), "nope").unwrap();
    let made = load_or_create_identity(&dir).unwrap();
    assert_eq!(load_or_create_identity(&dir).unwrap(), made);
}

#[test]
fn known_round_trips() {
    let dir = temp_dir("known");
    save_known(&dir, &[host(1), host(2)]).unwrap();
    assert_eq!(load_known(&dir), vec![host(1), host(2)]);
}

#[test]
fn remember_replaces_same_host() {
    let dir = temp_dir("remember");
    remember(&dir, host(1)).unwrap();
    let mut renamed = host(1);
    renamed.name = "Desk".to_owned();
    remember(&dir, renamed.clone()).unwrap();
    assert_eq!(load_known(&dir), vec![renamed]);
}

#[test]
fn forget_removes() {
    let dir = temp_dir("forget");
    save_known(&dir, &[host(1), host(2)]).unwrap();
    forget(&dir, host(1).host).unwrap();
    assert_eq!(load_known(&dir), vec![host(2)]);
    forget(&dir, host(9).host).unwrap();
    assert_eq!(load_known(&dir), vec![host(2)]);
}
