//! The real binary in `--console` mode serves the pipe.
#![cfg(windows)]

use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use mixlink_core::svc_ipc::{SvcPipe, SvcState};

#[test]
fn console_mode_serves_pipe() {
    let pipe = format!(r"\\.\pipe\mixlink-test-console-{}", std::process::id());
    let mut child = Command::new(env!("CARGO_BIN_EXE_mixlink-svc"))
        .args(["--console", "--pipe", &pipe])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    let connected = loop {
        match SvcPipe::connect(&pipe) {
            Ok((pipe, _)) => break Some(pipe),
            Err(_) if Instant::now() < deadline => thread::sleep(Duration::from_millis(50)),
            Err(_) => break None,
        }
    };
    let status = connected.map(|mut pipe| pipe.status());
    child.kill().unwrap();
    child.wait().unwrap();
    let (state, _) = status.expect("pipe never came up").unwrap();
    assert_eq!(state, SvcState::Idle);
}
