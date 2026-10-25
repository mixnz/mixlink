//! Process contract when stdin is closed (no launcher): the launcher mode closes with `Stopped`;
//! play mode (spec 0013) keeps running until its window is closed.

use std::io::{self, Read};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const STOPPED: &str = r#""reason":{"kind":"Stopped"}"#;

fn spawn(args: &[&str]) -> io::Result<Child> {
    // The FFmpeg DLLs must be found when the gate did not put them on PATH.
    let ffmpeg = PathBuf::from(std::env::var("FFMPEG_DIR").unwrap_or_default()).join("bin");
    let current = std::env::var_os("PATH").unwrap_or_default();
    let path = std::env::join_paths(std::iter::once(ffmpeg).chain(std::env::split_paths(&current)))
        .map_err(io::Error::other)?;
    Command::new(env!("CARGO_BIN_EXE_mixlink-stream"))
        .args(args)
        .env("PATH", path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
}

/// Waits up to `timeout`; kills the process if it is still running. Returns (exited, stdout).
fn wait_or_kill(mut child: Child, timeout: Duration) -> io::Result<(bool, String)> {
    let start = Instant::now();
    let mut exited = false;
    while start.elapsed() < timeout {
        if child.try_wait()?.is_some() {
            exited = true;
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
    if !exited {
        child.kill()?;
        child.wait()?;
    }
    let mut out = String::new();
    if let Some(mut stdout) = child.stdout.take() {
        stdout.read_to_string(&mut out)?;
    }
    Ok((exited, out))
}

#[test]
fn launcher_mode_closes_when_stdin_closes() {
    let (exited, out) = wait_or_kill(spawn(&[]).unwrap(), Duration::from_secs(10)).unwrap();
    assert!(exited, "still running without a launcher: {out}");
    assert!(out.contains(STOPPED), "{out}");
}

#[test]
fn play_mode_keeps_running_when_stdin_closes() {
    let file =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../crates/codec/tests/data/tiny.h264");
    let child = spawn(&["--play", file.to_str().unwrap(), "--loop"]).unwrap();
    let (_, out) = wait_or_kill(child, Duration::from_secs(3)).unwrap();
    // Without a hardware decoder (CI) the window closes with an error; it must never be the
    // "launcher gone" close.
    assert!(
        !out.contains(STOPPED),
        "play mode closed on stdin EOF: {out}"
    );
}
