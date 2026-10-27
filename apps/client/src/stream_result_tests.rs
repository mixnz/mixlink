use mixlink_core::ipc::{CloseReason, IPC_VERSION};

use super::*;

#[test]
fn closed_by_user() {
    let result = classify(Some(IPC_VERSION), Some(CloseReason::User), Some(0));
    assert_eq!(result, StreamResult::ClosedByUser);
}

#[test]
fn crash_without_closed() {
    assert_eq!(
        classify(Some(IPC_VERSION), None, Some(3)),
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
    let result = classify(Some(IPC_VERSION + 1), None, Some(1));
    assert_eq!(
        result,
        StreamResult::VersionMismatch {
            stream: IPC_VERSION + 1,
            launcher: IPC_VERSION
        }
    );
}

#[test]
fn stream_error() {
    let reason = CloseReason::Error {
        message: "x".to_owned(),
    };
    let result = classify(Some(IPC_VERSION), Some(reason), Some(0));
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

#[test]
fn classify_maps_new_close_reasons() {
    let cases = [
        (
            CloseReason::Rejected {
                reason: "busy".to_owned(),
            },
            StreamResult::Rejected {
                reason: "busy".to_owned(),
            },
        ),
        (
            CloseReason::HostEnded {
                reason: "shutdown".to_owned(),
            },
            StreamResult::HostEnded {
                reason: "shutdown".to_owned(),
            },
        ),
        (CloseReason::ConnectionLost, StreamResult::ConnectionLost),
        (CloseReason::HostUnreachable, StreamResult::HostUnreachable),
        (CloseReason::NotPaired, StreamResult::NotPaired),
    ];
    for (reason, expected) in cases {
        assert_eq!(classify(Some(IPC_VERSION), Some(reason), Some(0)), expected);
    }
}

#[test]
fn new_results_serialize_camel_case() {
    let json = serde_json::to_string(&StreamResult::ConnectionLost).unwrap();
    assert_eq!(json, r#"{"kind":"connectionLost"}"#);
}
