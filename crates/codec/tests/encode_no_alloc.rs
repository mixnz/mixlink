//! Hot-path check: `Encoder::encode` never allocates on the Rust heap once warm (FFmpeg's own
//! allocations use the C heap and are out of scope). Counting allocator as in `no_alloc.rs`.
#![cfg(windows)]

mod common;

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use common::hw_device_with_decoder;
use common::pixels::bgra_texture;
use mixlink_codec::{Encoder, EncoderConfig, VideoCodec, has_hardware_encoder};

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
}

#[global_allocator]
static GLOBAL: CountingAlloc = CountingAlloc;

#[test]
fn encode_does_not_allocate_when_warm() {
    let codec = VideoCodec::H264;
    let Some(device) = hw_device_with_decoder(codec).filter(|d| has_hardware_encoder(d, codec))
    else {
        eprintln!("skip: no NVENC encoder");
        return;
    };
    let texture = bgra_texture(&device, 256, 144, [10, 200, 30, 255]).unwrap();
    let config = EncoderConfig {
        codec,
        width: 256,
        height: 144,
        fps: 60,
        bitrate_kbps: 4000,
    };
    let mut encoder = Encoder::new(&device, &config).expect("encoder");
    for _ in 0..5 {
        encoder.encode(&texture, false).expect("warm-up encode");
    }
    let before = ALLOCATIONS.with(Cell::get);
    for i in 0..20 {
        encoder.encode(&texture, i == 10).expect("encode");
    }
    let after = ALLOCATIONS.with(Cell::get);
    assert_eq!(after - before, 0, "encode allocated on the Rust heap");
}
