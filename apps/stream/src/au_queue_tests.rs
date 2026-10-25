use std::thread;
use std::time::Duration;

use super::{SendError, channel};

#[test]
fn round_trip_keeps_bytes_and_stamp() {
    let (mut tx, mut rx) = channel(2, 64);
    tx.send(&[1, 2, 3]).unwrap();
    let au = rx.try_recv().unwrap();
    assert_eq!(au.data(), &[1, 2, 3]);
    assert!(au.received().raw() > 0);
}

#[test]
fn full_queue_returns_full_and_counts() {
    let (mut tx, mut rx) = channel(8, 64);
    for i in 0..8u8 {
        tx.send(&[i]).unwrap();
    }
    assert_eq!(tx.send(&[9]), Err(SendError::Full));
    assert_eq!(tx.counts().full, 1);
    drop(rx.try_recv().unwrap());
    tx.send(&[9]).unwrap();
}

#[test]
fn oversize_is_dropped_and_counted() {
    let (mut tx, mut rx) = channel(2, 1024);
    assert_eq!(tx.send(&[0; 1025]), Err(SendError::Oversize));
    assert_eq!(tx.counts().oversize, 1);
    tx.send(&[7; 1024]).unwrap();
    assert_eq!(rx.try_recv().unwrap().data().len(), 1024);
    assert!(rx.try_recv().is_none());
}

#[test]
fn fifo_across_threads() {
    let (mut tx, mut rx) = channel(8, 16);
    let producer = thread::spawn(move || {
        for i in 0..10_000u32 {
            while tx.send(&i.to_le_bytes()).is_err() {
                thread::yield_now();
            }
        }
    });
    rx.attach_current_thread();
    let mut next = 0u32;
    while next < 10_000 {
        match rx.try_recv() {
            Some(au) => {
                let got = u32::from_le_bytes(au.data().try_into().unwrap());
                assert_eq!(got, next);
                next += 1;
            }
            None => thread::park_timeout(Duration::from_millis(5)),
        }
    }
    producer.join().unwrap();
}
