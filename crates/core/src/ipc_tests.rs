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
        },
        StreamToLauncher::Closed {
            reason: CloseReason::Stopped,
        },
        StreamToLauncher::Closed {
            reason: CloseReason::Error {
                message: "boom".to_owned(),
            },
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
