use std::cell::RefCell;
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

fn handle(session: &mut Session, line: &[u8]) -> Reply {
    session.handle_line(line, Duration::ZERO, || false, &NoPairing)
}

fn greeted() -> Session {
    let mut session = Session::new();
    assert!(!handle(&mut session, &hello(PIPE_VERSION)).close);
    session
}

/// Records what the pipe asked of pairing.
#[derive(Default)]
struct FakePairing {
    prompt: Option<PairingPrompt>,
    cancelled: RefCell<u32>,
    removed: RefCell<Vec<DeviceId>>,
}

impl PairingPort for FakePairing {
    fn prompt(&self) -> Option<PairingPrompt> {
        self.prompt.clone()
    }

    fn paired(&self) -> Vec<PairedInfo> {
        vec![PairedInfo {
            device: DeviceId::from_bytes([1; 16]),
            name: "Laptop".to_owned(),
            paired_at: 5,
        }]
    }

    fn cancel(&self) {
        *self.cancelled.borrow_mut() += 1;
    }

    fn remove(&self, device: DeviceId) {
        self.removed.borrow_mut().push(device);
    }
}

#[test]
fn status_before_hello_is_refused() {
    let mut session = Session::new();
    assert!(is_error_and_close(&handle(&mut session, GET_STATUS)));
}

#[test]
fn hello_then_status() {
    let mut session = Session::new();
    let reply = handle(&mut session, &hello(PIPE_VERSION));
    assert_eq!(
        reply.msg,
        SvcToTray::Hello {
            pipe_version: PIPE_VERSION,
            svc_version: env!("CARGO_PKG_VERSION").to_owned()
        }
    );
    assert!(!reply.close);
    let reply = session.handle_line(GET_STATUS, Duration::from_secs(3), || false, &NoPairing);
    assert_eq!(
        reply.msg,
        SvcToTray::Status {
            state: SvcState::Idle,
            uptime_s: 3,
            network_public: false,
            pairing: None,
        }
    );
    assert!(!reply.close);
}

#[test]
fn old_pipe_version_closed() {
    let mut session = Session::new();
    let reply = handle(&mut session, &hello(1));
    assert!(matches!(
        reply.msg,
        SvcToTray::Hello {
            pipe_version: PIPE_VERSION,
            ..
        }
    ));
    assert!(reply.close);
}

#[test]
fn invalid_json_is_refused() {
    let mut session = Session::new();
    assert!(is_error_and_close(&handle(&mut session, b"{nope\n")));
}

#[test]
fn non_utf8_is_refused() {
    let mut session = Session::new();
    assert!(is_error_and_close(&handle(
        &mut session,
        &[0xff, 0xfe, b'\n']
    )));
}

#[test]
fn second_hello_is_refused() {
    let mut session = greeted();
    assert!(is_error_and_close(&handle(
        &mut session,
        &hello(PIPE_VERSION)
    )));
}

#[test]
fn too_long_reply_closes() {
    assert!(is_error_and_close(&too_long_reply()));
}

#[test]
fn status_reports_network_public() {
    let mut session = greeted();
    let reply = session.handle_line(GET_STATUS, Duration::ZERO, || true, &NoPairing);
    assert!(matches!(
        reply.msg,
        SvcToTray::Status {
            network_public: true,
            ..
        }
    ));
}

#[test]
fn hello_does_not_probe() {
    let mut session = Session::new();
    let reply = session.handle_line(
        &hello(PIPE_VERSION),
        Duration::ZERO,
        || panic!("network probed for Hello"),
        &NoPairing,
    );
    assert!(!reply.close);
}

#[test]
fn status_carries_prompt() {
    let prompt = PairingPrompt {
        device_name: "Laptop".to_owned(),
        pin: "012345".to_owned(),
        expires_s: 80,
    };
    let fake = FakePairing {
        prompt: Some(prompt.clone()),
        ..FakePairing::default()
    };
    let mut session = greeted();
    let reply = session.handle_line(GET_STATUS, Duration::ZERO, || false, &fake);
    assert!(matches!(reply.msg, SvcToTray::Status { pairing: Some(p), .. } if p == prompt));
}

#[test]
fn cancel_sends_command() {
    let fake = FakePairing::default();
    let mut session = greeted();
    let line = b"{\"type\":\"CancelPairing\"}\n";
    let reply = session.handle_line(line, Duration::ZERO, || false, &fake);
    assert_eq!(reply.msg, SvcToTray::Done);
    assert_eq!(*fake.cancelled.borrow(), 1);
}

#[test]
fn list_paired_has_no_keys() {
    let fake = FakePairing::default();
    let mut session = greeted();
    let line = b"{\"type\":\"ListPaired\"}\n";
    let reply = session.handle_line(line, Duration::ZERO, || false, &fake);
    let json = mixlink_core::ipc::encode_line(&reply.msg).unwrap();
    assert!(json.contains("Laptop"));
    assert!(!json.contains("key"), "{json}");
}

#[test]
fn remove_sends_command() {
    let fake = FakePairing::default();
    let mut session = greeted();
    let device = DeviceId::from_bytes([7; 16]);
    let line = format!("{{\"type\":\"RemovePaired\",\"device\":\"{device}\"}}\n");
    let reply = session.handle_line(line.as_bytes(), Duration::ZERO, || false, &fake);
    assert_eq!(reply.msg, SvcToTray::Done);
    assert_eq!(*fake.removed.borrow(), vec![device]);
}

#[test]
fn pairing_requests_before_hello_are_refused() {
    let fake = FakePairing::default();
    let mut session = Session::new();
    let line = b"{\"type\":\"CancelPairing\"}\n";
    let reply = session.handle_line(line, Duration::ZERO, || false, &fake);
    assert!(is_error_and_close(&reply));
    assert_eq!(*fake.cancelled.borrow(), 0);
}
