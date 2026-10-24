use std::net::{Ipv4Addr, SocketAddrV4};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::thread;
use std::time::{Duration, Instant};

use mixlink_core::clock::Ticks;

use super::*;
use crate::stats::{LaneCount, LaneCounters};

fn lane(capacity: usize) -> (Writer, Reader, Arc<LaneCounters>) {
    let counters = Arc::new(LaneCounters::default());
    let (writer, reader) = pair(capacity, Arc::new(Wake::default()), Arc::clone(&counters));
    (writer, reader, counters)
}

fn peer() -> SocketAddrV4 {
    SocketAddrV4::new(Ipv4Addr::new(10, 0, 0, 2), 5000)
}

fn commit_u32(writer: &mut Writer, value: u32) -> bool {
    match writer.try_reserve() {
        Some(mut slot) => {
            slot.buf()[..4].copy_from_slice(&value.to_le_bytes());
            slot.commit(4, peer(), Ticks::ZERO);
            true
        }
        None => false,
    }
}

#[test]
fn round_trip_keeps_bytes_peer_and_time() {
    let (mut writer, mut reader, _) = lane(4);
    let mut slot = writer.try_reserve().unwrap();
    slot.buf()[..5].copy_from_slice(&[1, 2, 3, 4, 5]);
    slot.commit(5, peer(), Ticks::from_raw(42));
    let got = reader.try_pop().unwrap();
    assert_eq!(got.data(), &[1, 2, 3, 4, 5]);
    assert_eq!(got.peer(), peer());
    assert_eq!(got.at(), Ticks::from_raw(42));
}

#[test]
fn full_lane_returns_none_and_counts() {
    let (mut writer, mut reader, counters) = lane(4);
    for i in 0..4 {
        assert!(commit_u32(&mut writer, i));
    }
    assert!(writer.try_reserve().is_none());
    assert_eq!(counters.snapshot(), LaneCount { passed: 4, full: 1 });
    assert!(reader.try_pop().is_some());
    assert!(commit_u32(&mut writer, 9));
}

#[test]
fn uncommitted_slot_is_reused() {
    let (mut writer, _reader, counters) = lane(1);
    for _ in 0..10 {
        assert!(writer.try_reserve().is_some());
    }
    assert_eq!(counters.full.load(Ordering::Relaxed), 0);
}

#[test]
fn fifo_across_threads() {
    let (mut writer, mut reader, _) = lane(64);
    let producer = thread::spawn(move || {
        for i in 0..100_000u32 {
            while !commit_u32(&mut writer, i) {
                std::hint::spin_loop();
            }
        }
    });
    for expected in 0..100_000u32 {
        let got = reader.recv_timeout(Duration::from_secs(1)).unwrap();
        let value = u32::from_le_bytes(got.data().try_into().unwrap());
        assert_eq!(value, expected);
    }
    producer.join().unwrap();
}

#[test]
fn recv_timeout_wakes_on_push() {
    let (mut writer, mut reader, _) = lane(4);
    let (go_tx, go_rx) = std::sync::mpsc::channel::<u32>();
    let producer = thread::spawn(move || {
        while let Ok(round) = go_rx.recv() {
            let spin = Instant::now();
            while spin.elapsed() < Duration::from_micros(u64::from(round % 201)) {
                std::hint::spin_loop();
            }
            assert!(commit_u32(&mut writer, round));
        }
    });
    for round in 0..1000u32 {
        go_tx.send(round).unwrap();
        let start = Instant::now();
        let got = reader.recv_timeout(Duration::from_secs(1));
        assert!(got.is_some(), "round {round} timed out");
        assert!(
            start.elapsed() < Duration::from_millis(20),
            "round {round}: {:?}",
            start.elapsed()
        );
    }
    drop(go_tx);
    producer.join().unwrap();
}

#[test]
fn recv_timeout_returns_none_when_idle() {
    let (_writer, mut reader, _) = lane(4);
    let start = Instant::now();
    assert!(reader.recv_timeout(Duration::from_millis(30)).is_none());
    let elapsed = start.elapsed();
    assert!(elapsed >= Duration::from_millis(30), "{elapsed:?}");
    assert!(elapsed < Duration::from_millis(200), "{elapsed:?}");
}

#[test]
fn datagram_is_first_1216_bytes() {
    let (mut writer, _reader, _) = lane(1);
    let mut slot = writer.try_reserve().unwrap();
    slot.buf().fill(0x11);
    slot.datagram().fill(0xAB);
    assert!(slot.buf()[..1216].iter().all(|&b| b == 0xAB));
    assert!(slot.buf()[1216..].iter().all(|&b| b == 0x11));
    assert_eq!(SLOT_LEN, 1232);
}

#[test]
fn huge_timeout_waits_without_overflow() {
    let (mut writer, mut reader, _) = lane(4);
    let producer = thread::spawn(move || {
        thread::sleep(Duration::from_millis(20));
        assert!(commit_u32(&mut writer, 7));
    });
    assert!(reader.recv_timeout(Duration::MAX).is_some());
    producer.join().unwrap();
}
