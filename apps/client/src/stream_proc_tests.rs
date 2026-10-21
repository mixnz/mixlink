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
    let result = spawn_at(Path::new("does-not-exist/mixlink-stream.exe"));
    assert!(matches!(result, Err(StartError::Spawn(_))));
}
