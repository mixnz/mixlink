//! Hot-path check: `Decoder::decode` and dropping a `DecodedFrame` never allocate on the Rust heap
//! once warm (FFmpeg's own pools use the C heap and are out of scope). Test-only counting
//! allocator (spec 0013); it delegates every call to `System`.

mod common;

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use common::{access_units, device_or_skip};
use mixlink_codec::{Decoder, VideoCodec};

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
fn decode_does_not_allocate_when_warm() {
    let codec = VideoCodec::H264;
    let Some(device) = device_or_skip(codec) else {
        return;
    };
    let mut decoder = Decoder::new(&device, codec).unwrap();
    let aus = access_units(codec).unwrap();
    for au in &aus[..5] {
        drop(decoder.decode(au).unwrap());
    }
    let mut frames = 0;
    let n = allocations_during(|| {
        for au in &aus[5..] {
            if let Ok(Some(frame)) = decoder.decode(au) {
                frames += 1;
                drop(frame);
            }
        }
    });
    assert_eq!(frames, 5);
    assert_eq!(n, 0, "decode allocated {n} times");
}
