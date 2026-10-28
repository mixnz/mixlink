use std::path::Path;
use std::sync::atomic::Ordering;

use super::*;

#[test]
fn second_start_is_rejected() {
    let state = StreamState::default();
    state.running.store(true, Ordering::SeqCst);
    assert!(matches!(try_begin(&state), Err(StartError::AlreadyRunning)));
}

#[test]
fn first_start_is_accepted() {
    let state = StreamState::default();
    assert!(try_begin(&state).is_ok());
    assert!(state.running.load(Ordering::SeqCst));
}

#[test]
fn missing_exe_is_spawn_failed() {
    let result = spawn_at(Path::new("does-not-exist/mixlink-stream.exe"), &[]);
    assert!(matches!(result, Err(StartError::Spawn(_))));
}

#[test]
fn connect_args_built() {
    let target = ConnectTarget {
        host: "00112233445566778899aabbccddeeff".parse().unwrap(),
        addr: "192.168.1.5:47801".parse().unwrap(),
    };
    assert_eq!(
        connect_args(&target),
        [
            "--connect",
            "00112233445566778899aabbccddeeff",
            "--addr",
            "192.168.1.5:47801"
        ]
    );
}

#[test]
fn target_needs_ipv4() {
    let id: DeviceId = "00112233445566778899aabbccddeeff".parse().unwrap();
    let v4 = target_of(id, "10.0.0.2".parse().unwrap(), 47801);
    assert_eq!(
        v4.map(|t| t.addr.to_string()),
        Some("10.0.0.2:47801".to_owned())
    );
    assert!(target_of(id, "fe80::1".parse().unwrap(), 47801).is_none());
}

#[test]
fn latency_kept_per_host() {
    let state = StreamState::default();
    assert!(state.last_latency().is_none());
    let latency = LatencySummary {
        frames: 10,
        spans: Vec::new(),
    };
    state.keep_latency("abc", latency.clone());
    let last = state.last_latency().unwrap();
    assert_eq!((last.host_id.as_str(), last.latency), ("abc", latency));
}
