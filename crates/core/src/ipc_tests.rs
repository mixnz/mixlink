use super::*;

#[test]
fn ready_encodes_exactly() {
    let line = encode_line(&StreamToLauncher::Ready { ipc_version: 1 }).unwrap();
    assert_eq!(line, "{\"type\":\"Ready\",\"ipc_version\":1}\n");
}

#[test]
fn closed_encodes_exactly() {
    let msg = StreamToLauncher::Closed {
        reason: CloseReason::User,
        latency: None,
    };
    let line = encode_line(&msg).unwrap();
    assert_eq!(
        line,
        "{\"type\":\"Closed\",\"reason\":{\"kind\":\"User\"}}\n"
    );
}

#[test]
fn stop_encodes_exactly() {
    assert_eq!(
        encode_line(&LauncherToStream::Stop).unwrap(),
        "{\"type\":\"Stop\"}\n"
    );
}

#[test]
fn every_message_round_trips() {
    let to_launcher = [
        StreamToLauncher::Ready {
            ipc_version: IPC_VERSION,
        },
        StreamToLauncher::Closed {
            reason: CloseReason::User,
            latency: None,
        },
        StreamToLauncher::Closed {
            reason: CloseReason::Stopped,
            latency: None,
        },
        StreamToLauncher::Closed {
            reason: CloseReason::Error {
                message: "boom".to_owned(),
            },
            latency: None,
        },
    ];
    for msg in to_launcher {
        let line = encode_line(&msg).unwrap();
        assert_eq!(decode_line::<StreamToLauncher>(&line).unwrap(), msg);
    }
    let line = encode_line(&LauncherToStream::Stop).unwrap();
    assert_eq!(
        decode_line::<LauncherToStream>(&line).unwrap(),
        LauncherToStream::Stop
    );
}

#[test]
fn decode_accepts_crlf() {
    let msg = decode_line::<LauncherToStream>("{\"type\":\"Stop\"}\r\n").unwrap();
    assert_eq!(msg, LauncherToStream::Stop);
}

#[test]
fn unknown_type_is_error() {
    let result = decode_line::<StreamToLauncher>("{\"type\":\"Bogus\"}");
    assert!(matches!(result, Err(IpcError::Decode(_))));
}

#[test]
fn bad_json_is_error() {
    let result = decode_line::<LauncherToStream>("not json");
    assert!(matches!(result, Err(IpcError::Decode(_))));
}

fn latency() -> LatencySummary {
    LatencySummary {
        frames: 1200,
        spans: vec![
            SpanSummary {
                name: "decode".to_owned(),
                p50_us: 1500,
                p99_us: 2300,
            },
            SpanSummary {
                name: "total".to_owned(),
                p50_us: 6000,
                p99_us: 8100,
            },
        ],
    }
}

#[test]
fn closed_with_latency_round_trip() {
    let msg = StreamToLauncher::Closed {
        reason: CloseReason::HostEnded {
            reason: "shutdown".to_owned(),
        },
        latency: Some(latency()),
    };
    let line = encode_line(&msg).unwrap();
    assert!(line.contains("\"p99Us\":8100"), "{line}");
    assert_eq!(decode_line::<StreamToLauncher>(&line).unwrap(), msg);
}

#[test]
fn closed_without_latency_parses() {
    let msg = decode_line::<StreamToLauncher>(
        "{\"type\":\"Closed\",\"reason\":{\"kind\":\"ConnectionLost\"}}",
    )
    .unwrap();
    assert_eq!(
        msg,
        StreamToLauncher::Closed {
            reason: CloseReason::ConnectionLost,
            latency: None,
        }
    );
}

#[test]
fn connected_round_trip() {
    let msgs = [
        StreamToLauncher::Connecting,
        StreamToLauncher::Connected {
            width: 2560,
            height: 1440,
            refresh_mhz: 120_000,
            codec: "hevc".to_owned(),
        },
        StreamToLauncher::Closed {
            reason: CloseReason::Rejected {
                reason: "busy".to_owned(),
            },
            latency: None,
        },
        StreamToLauncher::Closed {
            reason: CloseReason::HostUnreachable,
            latency: None,
        },
        StreamToLauncher::Closed {
            reason: CloseReason::NotPaired,
            latency: None,
        },
    ];
    for msg in msgs {
        let line = encode_line(&msg).unwrap();
        assert_eq!(decode_line::<StreamToLauncher>(&line).unwrap(), msg);
    }
}
