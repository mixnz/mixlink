//! Hot-path check: tx reserve / commit and rx receive / release never allocate.
//! Test-only counting allocator (as in `mixlink-proto`); it delegates every call to `System`.
//! Only the test thread is counted: the transport threads have their own thread-local counter.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::time::Duration;

use mixlink_net::{Transport, TransportConfig};
use mixlink_proto::channel::Channel;
use mixlink_proto::crypto::{Role, SessionKeys};

thread_local! {
    // `const` init and no destructor: reading it never allocates, so the allocator cannot recurse.
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}

struct CountingAlloc;

// SAFETY: every method forwards to `System` unchanged; the counter only adds a TLS increment.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.with(|c| c.set(c.get() + 1));
        // SAFETY: caller upholds `GlobalAlloc::alloc`'s contract, passed through as is.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: `ptr` came from `System.alloc` above with this `layout`.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOCATIONS.with(|c| c.set(c.get() + 1));
        // SAFETY: `ptr` came from `System` with `layout`; caller upholds the rest.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: CountingAlloc = CountingAlloc;

fn allocations_during(f: impl FnOnce()) -> usize {
    let before = ALLOCATIONS.with(Cell::get);
    f();
    ALLOCATIONS.with(Cell::get) - before
}

#[test]
fn counter_sees_allocations() {
    let n = allocations_during(|| drop(std::hint::black_box(vec![1u8; 64])));
    assert!(n >= 1);
}

#[test]
fn lane_hot_path_does_not_allocate() {
    let (_a, mut a_lanes) = Transport::start(TransportConfig::loopback()).unwrap();
    let (b, mut b_lanes) = Transport::start(TransportConfig::loopback()).unwrap();
    let mut tx = a_lanes.take_tx(Channel::Input).unwrap();
    let mut rx = b_lanes.take_rx(Channel::Input).unwrap();
    let to = b.local_addr();
    let round_trip = |tx: &mut mixlink_net::TxLane, rx: &mut mixlink_net::RxLane| -> bool {
        let Some(mut slot) = tx.try_reserve() else {
            return false;
        };
        let buf = slot.buf();
        buf[..16].copy_from_slice(&[1, 5, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0]);
        slot.commit(16, to);
        rx.recv_timeout(Duration::from_secs(1)).is_some()
    };
    for _ in 0..100 {
        assert!(round_trip(&mut tx, &mut rx));
    }
    let mut ok = 0;
    let n = allocations_during(|| {
        for _ in 0..1000 {
            if round_trip(&mut tx, &mut rx) {
                ok += 1;
            }
        }
    });
    assert_eq!(ok, 1000);
    assert_eq!(n, 0);
}

#[test]
fn sealed_lane_hot_path_does_not_allocate() {
    let (_a, mut a_lanes) = Transport::start(TransportConfig::loopback()).unwrap();
    let (b, mut b_lanes) = Transport::start(TransportConfig::loopback()).unwrap();
    let mut tx = a_lanes.take_tx(Channel::Input).unwrap();
    let mut rx = b_lanes.take_rx(Channel::Input).unwrap();
    let to = b.local_addr();
    let keys = SessionKeys {
        c2h: [1; 16],
        h2c: [2; 16],
    };
    let mut sealer = keys.sealer(Role::Client, Channel::Input);
    let mut opener = keys.opener(Role::Host, Channel::Input);
    let mut seq = 0u32;
    let mut round_trip = || -> bool {
        let Some(mut slot) = tx.try_reserve() else {
            return false;
        };
        seq += 1;
        let buf = slot.buf();
        buf[..16].copy_from_slice(&[1, 5, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0]);
        buf[4..8].copy_from_slice(&seq.to_le_bytes());
        if slot.seal_commit(&mut sealer, 16, to).is_err() {
            return false;
        }
        rx.recv_timeout(Duration::from_secs(1))
            .is_some_and(|mut got| got.open(&mut opener).is_ok())
    };
    for _ in 0..100 {
        assert!(round_trip());
    }
    let mut ok = 0;
    let n = allocations_during(|| {
        for _ in 0..1000 {
            if round_trip() {
                ok += 1;
            }
        }
    });
    assert_eq!(ok, 1000);
    assert_eq!(n, 0);
}
