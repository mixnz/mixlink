//! Host cursor handed from `stream-net` to the video thread (spec 0020): the position in atomics
//! (latest wins), new shapes through a small SPSC ring, and a wake so the video thread redraws even
//! when no video frame comes (hardware cursor moves make none).

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::thread::{self, Thread};
use std::time::{Duration, Instant};

use mixlink_render::cursor::{CursorDraw, CursorImage};
use rtrb::{Consumer, Producer, RingBuffer};

/// Shapes in flight to the video thread; more are dropped (the next position asks again).
const SHAPE_SLOTS: usize = 4;
/// Shortest time between two requests for the same missing shape.
pub const SHAPE_REQUEST_GAP: Duration = Duration::from_millis(100);

#[derive(Debug, Default)]
struct Shared {
    /// x (high 32 bits), y (low 32 bits), as u32 bit patterns.
    xy: AtomicU64,
    /// visible (bit 32), shape id (low 32 bits).
    flags: AtomicU64,
    changed: AtomicBool,
    wake: OnceLock<Thread>,
    /// The local pointer in stream pixels (spec 0021): `LOCAL_ACTIVE` | x (bits 32..48) | y (low 16 bits).
    local: AtomicU64,
}

/// Set in `Shared::local` while the local pointer is drawn instead of the host position.
const LOCAL_ACTIVE: u64 = 1 << 63;

/// Called on `stream-net` when the host cursor shows or hides (spec 0021: mouse mode).
pub struct VisibilityHook(Box<dyn FnMut(bool) + Send>);

impl std::fmt::Debug for VisibilityHook {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("VisibilityHook")
    }
}

/// The winit thread's end (spec 0021): in absolute mouse mode the host cursor is drawn at the local
/// pointer, without waiting for the host.
#[derive(Debug, Clone)]
pub struct LocalPointer {
    shared: Arc<Shared>,
}

impl LocalPointer {
    // HOT PATH
    pub fn set(&self, x: u16, y: u16) {
        let packed = LOCAL_ACTIVE | (u64::from(x) << 32) | u64::from(y);
        if self.shared.local.swap(packed, Ordering::Relaxed) != packed {
            notify(&self.shared);
        }
    }

    /// Back to the host position.
    pub fn clear(&self) {
        if self.shared.local.swap(0, Ordering::Relaxed) != 0 {
            notify(&self.shared);
        }
    }
}

/// One cursor change for the video thread.
#[derive(Debug)]
pub struct CursorChange {
    pub draw: CursorDraw,
    pub shape_id: u32,
    /// Newest shape that arrived since the last change, if any.
    pub new_shape: Option<(u32, CursorImage)>,
}

#[derive(Debug)]
pub struct CursorTx {
    shared: Arc<Shared>,
    shapes: Producer<(u32, CursorImage)>,
    /// Last shape id pushed; a position naming another one asks the host for it.
    have: u32,
    last_request: Option<(u32, Instant)>,
    visible: Option<bool>,
    on_visible: Option<VisibilityHook>,
}

#[derive(Debug)]
pub struct CursorRx {
    shared: Arc<Shared>,
    shapes: Consumer<(u32, CursorImage)>,
}

pub fn cursor_channel() -> (CursorTx, CursorRx) {
    let shared = Arc::new(Shared::default());
    let (tx, rx) = RingBuffer::new(SHAPE_SLOTS);
    (
        CursorTx {
            shared: Arc::clone(&shared),
            shapes: tx,
            have: 0,
            last_request: None,
            visible: None,
            on_visible: None,
        },
        CursorRx { shared, shapes: rx },
    )
}

impl CursorTx {
    /// Stores the position and wakes the video thread. Returns a shape id to ask the host for when the
    /// position names one this side does not have (at most every `SHAPE_REQUEST_GAP` per id).
    // HOT PATH
    pub fn set_position(&mut self, draw: CursorDraw, shape_id: u32, now: Instant) -> Option<u32> {
        let xy = (u64::from(draw.x as u32) << 32) | u64::from(draw.y as u32);
        let flags = (u64::from(draw.visible) << 32) | u64::from(shape_id);
        self.shared.xy.store(xy, Ordering::Relaxed);
        self.shared.flags.store(flags, Ordering::Relaxed);
        self.notify();
        if self.visible != Some(draw.visible) {
            self.visible = Some(draw.visible);
            if let Some(hook) = self.on_visible.as_mut() {
                (hook.0)(draw.visible);
            }
        }
        if shape_id == 0 || shape_id == self.have {
            return None;
        }
        let recent = self.last_request.is_some_and(|(id, at)| {
            id == shape_id && now.saturating_duration_since(at) < SHAPE_REQUEST_GAP
        });
        if recent {
            return None;
        }
        self.last_request = Some((shape_id, now));
        Some(shape_id)
    }

    /// A new shape (converted on this thread; allocation is fine on a shape change).
    pub fn push_shape(&mut self, shape_id: u32, image: CursorImage) {
        self.have = shape_id;
        if self.shapes.push((shape_id, image)).is_err() {
            log::debug!("cursor shape {shape_id} dropped: the video thread is behind");
        }
        self.notify();
    }

    fn notify(&self) {
        notify(&self.shared);
    }

    /// `hook` runs on this thread each time the host cursor shows or hides.
    pub fn on_visibility(&mut self, hook: Box<dyn FnMut(bool) + Send>) {
        self.on_visible = Some(VisibilityHook(hook));
    }
}

impl CursorRx {
    /// The calling thread is unparked on every change.
    pub fn attach_current_thread(&self) {
        let _ = self.shared.wake.set(thread::current());
    }

    pub fn local_pointer(&self) -> LocalPointer {
        LocalPointer {
            shared: Arc::clone(&self.shared),
        }
    }

    /// The latest state when anything changed since the last call.
    // HOT PATH
    pub fn take(&mut self) -> Option<CursorChange> {
        if !self.shared.changed.swap(false, Ordering::Acquire) {
            return None;
        }
        let mut new_shape = None;
        while let Ok(shape) = self.shapes.pop() {
            new_shape = Some(shape);
        }
        let xy = self.shared.xy.load(Ordering::Relaxed);
        let flags = self.shared.flags.load(Ordering::Relaxed);
        let local = self.shared.local.load(Ordering::Relaxed);
        let (x, y) = if local & LOCAL_ACTIVE != 0 {
            (i32::from((local >> 32) as u16), i32::from(local as u16))
        } else {
            ((xy >> 32) as u32 as i32, xy as u32 as i32)
        };
        Some(CursorChange {
            draw: CursorDraw {
                x,
                y,
                visible: flags >> 32 != 0,
            },
            shape_id: flags as u32,
            new_shape,
        })
    }
}

/// Marks a change and wakes the video thread.
fn notify(shared: &Shared) {
    shared.changed.store(true, Ordering::Release);
    if let Some(thread) = shared.wake.get() {
        thread.unpark();
    }
}

#[cfg(test)]
#[path = "cursor_state_tests.rs"]
mod tests;
