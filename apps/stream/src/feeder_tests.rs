use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

use mixlink_codec::VideoCodec;

use super::Feeder;
use crate::args::PlayArgs;
use crate::au_queue::channel;

fn tiny_h264(looping: bool) -> PlayArgs {
    PlayArgs {
        path: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../crates/codec/tests/data/tiny.h264"),
        codec: VideoCodec::H264,
        fps: 240,
        looping,
        measure: false,
    }
}

/// Receives AUs until `want` arrived or `timeout` passed; returns how many came.
fn receive(rx: &mut crate::au_queue::AuReceiver, want: usize, timeout: Duration) -> usize {
    let start = Instant::now();
    let mut got = 0;
    while got < want && start.elapsed() < timeout {
        match rx.try_recv() {
            Some(_) => got += 1,
            None => thread::sleep(Duration::from_millis(1)),
        }
    }
    got
}

#[test]
fn sends_every_au_once() {
    let (tx, mut rx) = channel(16, 64 * 1024);
    let feeder = Feeder::start(&tiny_h264(false), tx).unwrap();
    assert_eq!(receive(&mut rx, 10, Duration::from_secs(2)), 10);
    thread::sleep(Duration::from_millis(50));
    assert!(rx.try_recv().is_none(), "sent more than the file holds");
    feeder.stop();
}

#[test]
fn loop_restarts_the_file() {
    let (tx, mut rx) = channel(16, 64 * 1024);
    let feeder = Feeder::start(&tiny_h264(true), tx).unwrap();
    assert_eq!(receive(&mut rx, 25, Duration::from_secs(2)), 25);
    let started = Instant::now();
    feeder.stop();
    assert!(started.elapsed() < Duration::from_millis(100));
}

#[test]
fn paced_at_fps() {
    let (tx, mut rx) = channel(16, 64 * 1024);
    let started = Instant::now();
    let feeder = Feeder::start(&tiny_h264(false), tx).unwrap();
    assert_eq!(receive(&mut rx, 10, Duration::from_secs(2)), 10);
    // 10 AUs at 240 fps: the last one leaves 9 periods (37.5 ms) after the first.
    assert!(
        started.elapsed() >= Duration::from_millis(35),
        "{:?}",
        started.elapsed()
    );
    feeder.stop();
}

#[test]
fn missing_file_is_an_error() {
    let (tx, _rx) = channel(2, 16);
    let mut args = tiny_h264(false);
    args.path = PathBuf::from("does-not-exist.h264");
    assert!(Feeder::start(&args, tx).is_err());
}
