use std::io::Cursor;

use mixlink_core::ipc::StreamToLauncher;

use super::*;

fn events_for(input: &str) -> Vec<StreamEvent> {
    let mut events = Vec::new();
    read_commands(Cursor::new(input), |event| events.push(event));
    events
}

#[test]
fn stop_line_emits_stop() {
    let events = events_for("{\"type\":\"Stop\"}\n");
    assert_eq!(events, [StreamEvent::Stop, StreamEvent::LauncherGone]);
}

#[test]
fn eof_emits_launcher_gone() {
    assert_eq!(events_for(""), [StreamEvent::LauncherGone]);
}

#[test]
fn garbage_line_is_skipped() {
    let events = events_for("junk\n{\"type\":\"Stop\"}\n");
    assert_eq!(events, [StreamEvent::Stop, StreamEvent::LauncherGone]);
}

#[test]
fn send_writes_one_line() {
    let mut out = Vec::new();
    send(&mut out, &StreamToLauncher::Ready { ipc_version: 1 }).unwrap();
    assert_eq!(out, b"{\"type\":\"Ready\",\"ipc_version\":1}\n");
}
