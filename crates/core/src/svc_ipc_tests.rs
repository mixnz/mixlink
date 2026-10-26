use std::io::Cursor;

use super::*;
use crate::ipc::{IpcError, decode_line, encode_line};

#[test]
fn every_message_round_trips() {
    for msg in [TrayToSvc::Hello { pipe_version: 1 }, TrayToSvc::GetStatus] {
        let line = encode_line(&msg).unwrap();
        assert_eq!(decode_line::<TrayToSvc>(&line).unwrap(), msg);
    }
    for msg in [
        SvcToTray::Hello {
            pipe_version: 1,
            svc_version: "0.1.0".to_owned(),
        },
        SvcToTray::Status {
            state: SvcState::Idle,
            uptime_s: 42,
            network_public: true,
        },
        SvcToTray::Error {
            message: "bad".to_owned(),
        },
    ] {
        let line = encode_line(&msg).unwrap();
        assert_eq!(decode_line::<SvcToTray>(&line).unwrap(), msg);
    }
}

#[test]
fn status_encodes_exactly() {
    let msg = SvcToTray::Status {
        state: SvcState::Idle,
        uptime_s: 3,
        network_public: false,
    };
    assert_eq!(
        encode_line(&msg).unwrap(),
        "{\"type\":\"Status\",\"state\":\"idle\",\"uptime_s\":3,\"network_public\":false}\n"
    );
}

#[test]
fn unknown_type_is_error() {
    let result = decode_line::<TrayToSvc>(r#"{"type":"Nope"}"#);
    assert!(matches!(result, Err(IpcError::Decode(_))));
}

#[test]
fn crlf_line_decodes() {
    let msg = decode_line::<TrayToSvc>("{\"type\":\"GetStatus\"}\r\n").unwrap();
    assert_eq!(msg, TrayToSvc::GetStatus);
}

#[test]
fn read_line_limited_reads_one_line() {
    let mut reader = Cursor::new(b"abc\ndef\n".to_vec());
    let mut buf = Vec::new();
    let read = read_line_limited(&mut reader, &mut buf, MAX_LINE).unwrap();
    assert_eq!(read, LineRead::Line);
    assert_eq!(buf, b"abc\n");
    let read = read_line_limited(&mut reader, &mut buf, MAX_LINE).unwrap();
    assert_eq!(read, LineRead::Line);
    assert_eq!(buf, b"def\n");
}

#[test]
fn read_line_limited_eof() {
    let mut reader = Cursor::new(Vec::new());
    let mut buf = vec![1, 2, 3];
    let read = read_line_limited(&mut reader, &mut buf, MAX_LINE).unwrap();
    assert_eq!(read, LineRead::Eof);
    assert!(buf.is_empty());
}

#[test]
fn read_line_limited_too_long() {
    let mut data = vec![b'a'; MAX_LINE + 1];
    data.push(b'\n');
    let mut reader = Cursor::new(data);
    let mut buf = Vec::new();
    let read = read_line_limited(&mut reader, &mut buf, MAX_LINE).unwrap();
    assert_eq!(read, LineRead::TooLong);
}

#[test]
fn read_line_limited_exactly_max_is_line() {
    let mut data = vec![b'a'; MAX_LINE - 1];
    data.push(b'\n');
    let mut reader = Cursor::new(data);
    let mut buf = Vec::new();
    let read = read_line_limited(&mut reader, &mut buf, MAX_LINE).unwrap();
    assert_eq!(read, LineRead::Line);
    assert_eq!(buf.len(), MAX_LINE);
}

#[test]
fn read_line_limited_last_line_without_newline() {
    let mut reader = Cursor::new(b"tail".to_vec());
    let mut buf = Vec::new();
    let read = read_line_limited(&mut reader, &mut buf, MAX_LINE).unwrap();
    assert_eq!(read, LineRead::Line);
    assert_eq!(buf, b"tail");
}

#[cfg(windows)]
#[test]
fn connect_missing_pipe_is_not_running() {
    let name = format!(r"\\.\pipe\mixlink-test-none-{}", std::process::id());
    let result = SvcPipe::connect(&name);
    assert!(matches!(result, Err(SvcPipeError::NotRunning)));
}

#[test]
fn status_without_network_public_decodes_false() {
    let msg = decode_line::<SvcToTray>(r#"{"type":"Status","state":"idle","uptime_s":7}"#).unwrap();
    assert_eq!(
        msg,
        SvcToTray::Status {
            state: SvcState::Idle,
            uptime_s: 7,
            network_public: false,
        }
    );
}

#[test]
fn status_report_from_message() {
    let msg = SvcToTray::Status {
        state: SvcState::Idle,
        uptime_s: 9,
        network_public: true,
    };
    assert_eq!(
        StatusReport::from_status(msg),
        Some(StatusReport {
            state: SvcState::Idle,
            uptime_s: 9,
            network_public: true,
        })
    );
}
