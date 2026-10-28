//! Hot-path check: `Fragments::next_into`, `Reassembler::push`, `Sealer::seal`, `Opener::open` and the input packet never allocate.
//! Test-only counting allocator (spec 0010); it delegates every call to `System`.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use mixlink_core::ids::FrameId;
use mixlink_proto::channel::Channel;
use mixlink_proto::crypto::{KEY_LEN, Role, SessionKeys, TAG_LEN};
use mixlink_proto::header::{Flags, MAX_DATAGRAM, PacketHeader};
use mixlink_proto::input::{INPUT_LEN, InputEvent, InputPacket, MAX_EVENTS, MouseMode};
use mixlink_proto::packetizer::Packetizer;
use mixlink_proto::reassembler::{Push, Reassembler, ReassemblerConfig};

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
fn push_and_next_into_do_not_allocate() {
    let frame = vec![0x5Au8; 1 << 20];
    let single = [7u8; 40];
    let mut packetizer = Packetizer::new(Channel::Video);
    let mut reassembler = Reassembler::new(ReassemblerConfig::for_channel(Channel::Video));
    let mut buf = [0u8; MAX_DATAGRAM];
    let mut completed = 0;

    let n = allocations_during(|| {
        for (id, message) in [(1u32, &frame[..]), (2, &single[..])] {
            let Ok(mut fragments) = packetizer.packetize(FrameId::new(id), Flags::empty(), message)
            else {
                return;
            };
            while let Some(len) = fragments.next_into(&mut buf) {
                if let Ok((header, payload)) = PacketHeader::decode(&buf[..len])
                    && let Push::Complete(_) = reassembler.push(&header, payload)
                {
                    completed += 1;
                }
            }
        }
    });

    assert_eq!(completed, 2);
    assert_eq!(n, 0);
}

#[test]
fn seal_and_open_do_not_allocate() {
    let keys = SessionKeys {
        c2h: [3; KEY_LEN],
        h2c: [4; KEY_LEN],
    };
    let mut sealer = keys.sealer(Role::Client, Channel::Video);
    let mut opener = keys.opener(Role::Host, Channel::Video);
    let frame = vec![0x5Au8; 64 * 1024];
    let mut packetizer = Packetizer::new(Channel::Video);
    let mut buf = [0u8; MAX_DATAGRAM + TAG_LEN];
    let mut opened = 0;

    let n = allocations_during(|| {
        let Ok(mut fragments) = packetizer.packetize(FrameId::new(1), Flags::empty(), &frame)
        else {
            return;
        };
        while let Some(len) = buf
            .first_chunk_mut::<MAX_DATAGRAM>()
            .and_then(|datagram| fragments.next_into(datagram))
        {
            if let Ok(total) = sealer.seal(&mut buf, len)
                && opener.open(&mut buf[..total]).is_ok()
            {
                opened += 1;
            }
        }
    });

    assert_eq!(opened, 55);
    assert_eq!(n, 0);
}

#[test]
fn input_encode_and_decode_do_not_allocate() {
    let packet = InputPacket {
        seq: 9,
        mode: MouseMode::Relative,
        buttons: 1,
        abs_x: 0,
        abs_y: 0,
        rel_x: -3,
        rel_y: 4,
        wheel: 120,
        hwheel: 0,
        first_event: 1,
        count: 1,
        events: [InputEvent {
            code: 0x04,
            down: true,
        }; MAX_EVENTS],
        keys: [0; 32],
    };
    let mut out = [0u8; INPUT_LEN];
    let mut decoded = None;
    let n = allocations_during(|| {
        packet.encode(&mut out);
        decoded = InputPacket::decode(&out).ok();
    });
    assert!(decoded.is_some());
    assert_eq!(n, 0);
}
