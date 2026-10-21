use mixlink_core::ipc::CloseReason;

use super::*;

#[test]
fn closed_by_user() {
    let result = classify(Some(1), Some(CloseReason::User), Some(0));
    assert_eq!(result, StreamResult::ClosedByUser);
}

#[test]
fn crash_without_closed() {
    assert_eq!(
        classify(Some(1), None, Some(3)),
        StreamResult::Crashed { code: Some(3) }
    );
}

#[test]
fn crash_before_ready() {
    assert_eq!(
        classify(None, None, None),
        StreamResult::Crashed { code: None }
    );
}

#[test]
fn version_mismatch_wins() {
    let result = classify(Some(2), None, Some(1));
    assert_eq!(
        result,
        StreamResult::VersionMismatch {
            stream: 2,
            launcher: 1
        }
    );
}

#[test]
fn stream_error() {
    let reason = CloseReason::Error {
        message: "x".to_owned(),
    };
    let result = classify(Some(1), Some(reason), Some(0));
    assert_eq!(
        result,
        StreamResult::StreamError {
            message: "x".to_owned()
        }
    );
}

#[test]
fn serde_shape() {
    let crashed = serde_json::to_string(&StreamResult::Crashed { code: Some(3) }).unwrap();
    assert_eq!(crashed, "{\"kind\":\"crashed\",\"code\":3}");
    let mismatch = serde_json::to_string(&StreamResult::VersionMismatch {
        stream: 2,
        launcher: 1,
    })
    .unwrap();
    assert_eq!(
        mismatch,
        "{\"kind\":\"versionMismatch\",\"stream\":2,\"launcher\":1}"
    );
}
