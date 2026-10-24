use std::time::Duration;

use mixlink_core::svc_ipc::{SvcState, SvcToTray};

use super::*;

fn hello(version: u32) -> Vec<u8> {
    format!("{{\"type\":\"Hello\",\"pipe_version\":{version}}}\n").into_bytes()
}

const GET_STATUS: &[u8] = b"{\"type\":\"GetStatus\"}\n";

fn is_error_and_close(reply: &Reply) -> bool {
    matches!(reply.msg, SvcToTray::Error { .. }) && reply.close
}

#[test]
fn status_before_hello_is_refused() {
    let mut session = Session::new();
    assert!(is_error_and_close(
        &session.handle_line(GET_STATUS, Duration::ZERO)
    ));
}

#[test]
fn hello_then_status() {
    let mut session = Session::new();
    let reply = session.handle_line(&hello(1), Duration::ZERO);
    assert_eq!(
        reply.msg,
        SvcToTray::Hello {
            pipe_version: 1,
            svc_version: env!("CARGO_PKG_VERSION").to_owned()
        }
    );
    assert!(!reply.close);
    let reply = session.handle_line(GET_STATUS, Duration::from_secs(3));
    assert_eq!(
        reply.msg,
        SvcToTray::Status {
            state: SvcState::Idle,
            uptime_s: 3
        }
    );
    assert!(!reply.close);
}

#[test]
fn other_version_gets_hello_and_close() {
    let mut session = Session::new();
    let reply = session.handle_line(&hello(2), Duration::ZERO);
    assert!(matches!(
        reply.msg,
        SvcToTray::Hello {
            pipe_version: 1,
            ..
        }
    ));
    assert!(reply.close);
}

#[test]
fn invalid_json_is_refused() {
    let mut session = Session::new();
    assert!(is_error_and_close(
        &session.handle_line(b"{nope\n", Duration::ZERO)
    ));
}

#[test]
fn non_utf8_is_refused() {
    let mut session = Session::new();
    assert!(is_error_and_close(
        &session.handle_line(&[0xff, 0xfe, b'\n'], Duration::ZERO)
    ));
}

#[test]
fn second_hello_is_refused() {
    let mut session = Session::new();
    session.handle_line(&hello(1), Duration::ZERO);
    assert!(is_error_and_close(
        &session.handle_line(&hello(1), Duration::ZERO)
    ));
}

#[test]
fn too_long_reply_closes() {
    assert!(is_error_and_close(&too_long_reply()));
}
