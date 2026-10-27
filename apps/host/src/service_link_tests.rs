use std::io;

use mixlink_core::svc_ipc::SvcPipeError;

use super::*;

fn connected(version: &str, uptime_s: u64) -> LinkState {
    LinkState::Connected {
        svc_version: version.to_owned(),
        uptime_s,
        network_public: false,
        pairing: None,
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
        r#"{"kind":"connected","svcVersion":"0.1.0","uptimeS":7,"networkPublic":false,"pairing":null}"#
    );
}

#[test]
fn tracker_emits_on_network_public_change() {
    let mut tracker = LinkTracker::default();
    tracker.update(connected("0.1.0", 1));
    let public = LinkState::Connected {
        svc_version: "0.1.0".to_owned(),
        uptime_s: 2,
        network_public: true,
        pairing: None,
    };
    assert_eq!(tracker.update(public.clone()), Some(public));
}

fn pairing(pin: &str, expires_s: u32) -> LinkState {
    LinkState::Connected {
        svc_version: "0.1.0".to_owned(),
        uptime_s: 1,
        network_public: false,
        pairing: Some(PromptState {
            device_name: "Laptop".to_owned(),
            pin: pin.to_owned(),
            expires_s,
        }),
    }
}

#[test]
fn prompt_change_emits() {
    let mut tracker = LinkTracker::default();
    tracker.update(connected("0.1.0", 1));
    assert!(tracker.update(pairing("123456", 120)).is_some());
    assert!(
        tracker.update(pairing("123456", 119)).is_none(),
        "countdown alone"
    );
    assert!(tracker.update(pairing("654321", 120)).is_some());
    assert!(
        tracker.update(connected("0.1.0", 3)).is_some(),
        "prompt closed"
    );
}

#[test]
fn untrusted_state_serializes() {
    assert_eq!(
        serde_json::to_string(&LinkState::Untrusted).unwrap(),
        r#"{"kind":"untrusted"}"#
    );
    assert_eq!(
        tooltip(&LinkState::Untrusted),
        strings::TRAY_TOOLTIP_UNTRUSTED
    );
}
