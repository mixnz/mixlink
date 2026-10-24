use std::io;

use mixlink_core::svc_ipc::SvcPipeError;

use super::*;

fn connected(version: &str, uptime_s: u64) -> LinkState {
    LinkState::Connected {
        svc_version: version.to_owned(),
        uptime_s,
    }
}

#[test]
fn first_update_emits() {
    let mut tracker = LinkTracker::default();
    assert_eq!(
        tracker.update(LinkState::NotRunning),
        Some(LinkState::NotRunning)
    );
}

#[test]
fn same_state_new_uptime_does_not_emit() {
    let mut tracker = LinkTracker::default();
    tracker.update(connected("0.1.0", 1));
    assert_eq!(tracker.update(connected("0.1.0", 2)), None);
}

#[test]
fn connected_to_not_running_emits() {
    let mut tracker = LinkTracker::default();
    tracker.update(connected("0.1.0", 1));
    assert_eq!(
        tracker.update(LinkState::NotRunning),
        Some(LinkState::NotRunning)
    );
}

#[test]
fn not_running_to_connected_emits() {
    let mut tracker = LinkTracker::default();
    tracker.update(LinkState::NotRunning);
    assert_eq!(
        tracker.update(connected("0.1.0", 0)),
        Some(connected("0.1.0", 0))
    );
}

#[test]
fn version_change_emits() {
    let mut tracker = LinkTracker::default();
    tracker.update(connected("0.1.0", 5));
    assert_eq!(
        tracker.update(connected("0.2.0", 0)),
        Some(connected("0.2.0", 0))
    );
}

#[test]
fn state_from_error_maps_version_mismatch() {
    let error = SvcPipeError::VersionMismatch {
        ours: 1,
        theirs: 2,
        svc_version: "0.2.0".to_owned(),
    };
    assert_eq!(
        state_from_error(&error),
        LinkState::VersionMismatch {
            svc_version: "0.2.0".to_owned()
        }
    );
    assert_eq!(
        state_from_error(&SvcPipeError::NotRunning),
        LinkState::NotRunning
    );
}

#[test]
fn request_error_drops_to_not_running() {
    let mut tracker = LinkTracker::default();
    tracker.update(connected("0.1.0", 3));
    let state = state_from_error(&SvcPipeError::Io(io::ErrorKind::UnexpectedEof.into()));
    assert_eq!(state, LinkState::NotRunning);
    assert_eq!(tracker.update(state), Some(LinkState::NotRunning));
}

#[test]
fn link_state_serializes_with_kind() {
    assert_eq!(
        serde_json::to_string(&LinkState::NotRunning).unwrap(),
        r#"{"kind":"notRunning"}"#
    );
    assert_eq!(
        serde_json::to_string(&connected("0.1.0", 7)).unwrap(),
        r#"{"kind":"connected","svcVersion":"0.1.0","uptimeS":7}"#
    );
}
