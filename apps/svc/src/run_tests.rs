use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, mpsc};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use mixlink_core::config::DisplaySettings;
use mixlink_core::svc_ipc::{MAX_LINE, SvcPipe, SvcPipeError, SvcState};

use super::*;

static NEXT: AtomicU32 = AtomicU32::new(0);

fn unique_pipe() -> String {
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    format!(r"\\.\pipe\mixlink-test-{}-{n}", std::process::id())
}

struct Server {
    pipe: String,
    stop: Arc<StopSignal>,
    handle: JoinHandle<Result<(), SvcError>>,
}

impl Server {
    fn start() -> Server {
        Server::start_with(None)
    }

    fn start_with(display: Option<DisplaySettings>) -> Server {
        let pipe = unique_pipe();
        let stop = Arc::new(StopSignal::new());
        let (ready_tx, ready_rx) = mpsc::channel();
        let opts = RunOptions {
            pipe: pipe.clone(),
            display,
            input: false,
            discovery: None,
        };
        let thread_stop = Arc::clone(&stop);
        let handle = thread::spawn(move || {
            run(&thread_stop, &opts, || {
                ready_tx.send(()).unwrap();
            })
        });
        ready_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        Server { pipe, stop, handle }
    }

    fn shutdown(self) {
        self.stop.stop();
        self.handle.join().unwrap().unwrap();
    }
}

fn raw_client(pipe: &str) -> File {
    OpenOptions::new()
        .read(true)
        .write(true)
        .open(pipe)
        .unwrap()
}

/// Reads one reply line, then expects the server to close.
fn read_reply_then_eof(file: &File) -> String {
    let mut reader = BufReader::new(file);
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    let mut rest = String::new();
    // A closed pipe reads as EOF or "broken pipe".
    let more = reader.read_line(&mut rest).unwrap_or(0);
    assert_eq!(more, 0, "connection still open, got {rest:?}");
    line
}

#[test]
fn round_trip_hello_and_status() {
    let server = Server::start();
    let (mut pipe, version) = SvcPipe::connect(&server.pipe).unwrap();
    assert_eq!(version, env!("CARGO_PKG_VERSION"));
    let (state, first) = pipe.status().unwrap();
    assert_eq!(state, SvcState::Idle);
    thread::sleep(Duration::from_millis(1100));
    let (_, second) = pipe.status().unwrap();
    assert!(second > first, "uptime {first} -> {second}");
    drop(pipe);
    server.shutdown();
}

#[test]
fn too_long_line_gets_error_and_close() {
    let server = Server::start();
    let mut file = raw_client(&server.pipe);
    let mut data = vec![b'a'; MAX_LINE + 1];
    data.push(b'\n');
    file.write_all(&data).unwrap();
    assert!(read_reply_then_eof(&file).contains("\"Error\""));
    server.shutdown();
}

#[test]
fn bad_json_and_status_before_hello_close() {
    let server = Server::start();
    let (mut good, _) = SvcPipe::connect(&server.pipe).unwrap();
    for line in [&b"{nope\n"[..], b"{\"type\":\"GetStatus\"}\n"] {
        let mut file = raw_client(&server.pipe);
        file.write_all(line).unwrap();
        assert!(read_reply_then_eof(&file).contains("\"Error\""));
    }
    assert_eq!(good.status().unwrap().0, SvcState::Idle);
    drop(good);
    server.shutdown();
}

#[test]
fn version_mismatch_reported() {
    let server = Server::start();
    let mut file = raw_client(&server.pipe);
    file.write_all(b"{\"type\":\"Hello\",\"pipe_version\":2}\n")
        .unwrap();
    let reply = read_reply_then_eof(&file);
    assert!(reply.contains("\"Hello\"") && reply.contains("\"pipe_version\":1"));
    server.shutdown();
}

#[test]
fn second_server_same_name_is_pipe_taken() {
    let server = Server::start();
    let stop = Arc::new(StopSignal::new());
    let opts = RunOptions {
        pipe: server.pipe.clone(),
        display: None,
        input: false,
        discovery: None,
    };
    let result = run(&stop, &opts, || {});
    assert!(matches!(result, Err(SvcError::PipeTaken)), "{result:?}");
    server.shutdown();
}

#[test]
fn stop_returns_within_one_second() {
    let server = Server::start();
    let (_idle, _) = SvcPipe::connect(&server.pipe).unwrap();
    let started = Instant::now();
    server.shutdown();
    assert!(started.elapsed() < Duration::from_secs(1));
}

#[test]
fn silent_client_does_not_block_others() {
    let server = Server::start();
    let _silent = raw_client(&server.pipe);
    let (mut pipe, _) = SvcPipe::connect(&server.pipe).unwrap();
    assert_eq!(pipe.status().unwrap().0, SvcState::Idle);
    drop(pipe);
    server.shutdown();
}

#[test]
fn fifth_client_busy_until_slot_frees() {
    let server = Server::start();
    let mut clients: Vec<SvcPipe> = (0..4)
        .map(|_| SvcPipe::connect(&server.pipe).unwrap().0)
        .collect();
    assert!(matches!(
        SvcPipe::connect(&server.pipe),
        Err(SvcPipeError::NotRunning)
    ));
    clients.pop();
    let deadline = Instant::now() + Duration::from_secs(2);
    let fifth = loop {
        match SvcPipe::connect(&server.pipe) {
            Ok((pipe, _)) => break pipe,
            Err(SvcPipeError::NotRunning) if Instant::now() < deadline => {
                thread::sleep(Duration::from_millis(20));
            }
            Err(error) => panic!("fifth client: {error}"),
        }
    };
    drop(fifth);
    drop(clients);
    server.shutdown();
}

/// Without the driver (or without admin rights) the display thread retries or gives up; either
/// way a stop is not held up by it.
#[test]
fn display_thread_stops_while_waiting() {
    let server = Server::start_with(Some(DisplaySettings::default()));
    thread::sleep(Duration::from_millis(200));
    let started = Instant::now();
    server.shutdown();
    assert!(started.elapsed() < Duration::from_secs(1));
}
