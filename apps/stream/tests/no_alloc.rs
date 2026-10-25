//! Hot-path check: AU queue send / receive / release and `FrameStats::record` never allocate
//! once the queue exists. Test-only counting allocator (spec 0013); it delegates to `System`.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use mixlink_core::ids::FrameId;
use mixlink_core::stage::{Stage, StageStamps};
use mixlink_stream::au_queue::{AU_SLOTS, channel};
use mixlink_stream::stats::FrameStats;

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
fn queue_and_stats_do_not_allocate() {
    let (mut tx, mut rx) = channel(AU_SLOTS, 4096);
    let mut stats = FrameStats::new(1000);
    let au = [0x42u8; 3000];
    let mut received = 0;
    let n = allocations_during(|| {
        for i in 0..1000u32 {
            if tx.send(&au).is_ok()
                && let Some(got) = rx.try_recv()
            {
                received += usize::from(got.data().len() == au.len());
            }
            let mut stamps = StageStamps::new(FrameId::new(i));
            stamps.mark(Stage::Received);
            stamps.mark(Stage::Presented);
            stats.record(&stamps);
        }
    });
    assert_eq!(received, 1000);
    assert_eq!(n, 0, "allocated {n} times");
}
