//! `IddSource::open` without a frame share for the connector (CI, a dev box with fewer monitors).

#![cfg(windows)]

use mixlink_capture::{CaptureError, IddSource};

#[test]
fn missing_connector_is_no_frame_share() {
    // Connector 3 exists only with four MixLink monitors added; nothing creates it here.
    match IddSource::open(3) {
        Err(CaptureError::NoFrameShare { connector: 3 }) => {}
        Err(other) => panic!("expected NoFrameShare, got {other}"),
        Ok(_) => eprintln!("skip: connector 3 has a frame share on this machine"),
    }
}

#[test]
fn open_rejects_connector_out_of_range() {
    assert!(matches!(
        IddSource::open(4),
        Err(CaptureError::NoFrameShare { connector: 4 })
    ));
}
