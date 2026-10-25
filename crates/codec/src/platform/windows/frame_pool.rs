//! Preallocated `AVFrame`s handed out without locking or allocating (`// HOT PATH`).
//! Shared by the decoder and every frame it returned, so a frame may outlive the decoder.

use std::ptr;
use std::sync::Arc;
use std::sync::atomic::{AtomicPtr, Ordering};

use ffmpeg_sys_next as ff;

use crate::CodecError;

/// Frames the caller may hold at once, plus one being decoded and one spare.
pub(crate) const POOL_FRAMES: usize = crate::decoder::EXTRA_SURFACES + 2;

#[derive(Debug)]
pub(crate) struct FramePool {
    slots: [AtomicPtr<ff::AVFrame>; POOL_FRAMES],
}

impl FramePool {
    pub(crate) fn new() -> Result<Arc<Self>, CodecError> {
        let pool = Arc::new(Self {
            slots: std::array::from_fn(|_| AtomicPtr::new(ptr::null_mut())),
        });
        for slot in &pool.slots {
            // SAFETY: plain allocation, null-checked; owned by the pool from here on.
            let frame = unsafe { ff::av_frame_alloc() };
            if frame.is_null() {
                return Err(CodecError::OutOfMemory);
            }
            slot.store(frame, Ordering::Release);
        }
        Ok(pool)
    }

    /// An empty frame, or `None` when the caller holds them all.
    pub(crate) fn take(self: &Arc<Self>) -> Option<PooledFrame> {
        self.slots.iter().find_map(|slot| {
            let frame = slot.swap(ptr::null_mut(), Ordering::AcqRel);
            (!frame.is_null()).then(|| PooledFrame {
                pool: Arc::clone(self),
                frame,
            })
        })
    }

    fn give_back(&self, frame: *mut ff::AVFrame) {
        for slot in &self.slots {
            if slot
                .compare_exchange(ptr::null_mut(), frame, Ordering::AcqRel, Ordering::Relaxed)
                .is_ok()
            {
                return;
            }
        }
        // Unreachable: there are as many slots as frames. Free rather than leak.
        let mut frame = frame;
        // SAFETY: the frame is owned here and no longer referenced anywhere.
        unsafe { ff::av_frame_free(&mut frame) };
    }
}

impl Drop for FramePool {
    fn drop(&mut self) {
        for slot in &self.slots {
            let mut frame = slot.swap(ptr::null_mut(), Ordering::AcqRel);
            // SAFETY: every stored frame is owned by the pool; av_frame_free accepts null.
            unsafe { ff::av_frame_free(&mut frame) };
        }
    }
}

/// One frame out of the pool; unreferenced and returned on drop.
#[derive(Debug)]
pub(crate) struct PooledFrame {
    pool: Arc<FramePool>,
    frame: *mut ff::AVFrame,
}

// SAFETY: the AVFrame is reachable only through this value; FFmpeg frames and the D3D11 resources
// they reference may be released from any thread.
unsafe impl Send for PooledFrame {}

impl PooledFrame {
    pub(crate) fn as_ptr(&self) -> *mut ff::AVFrame {
        self.frame
    }
}

impl Drop for PooledFrame {
    fn drop(&mut self) {
        // SAFETY: the frame is live and owned by this value; unref drops its buffers (and with the
        // last one the hw frames context and device).
        unsafe { ff::av_frame_unref(self.frame) };
        self.pool.give_back(self.frame);
    }
}
