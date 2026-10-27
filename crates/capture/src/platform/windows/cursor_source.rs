//! Reader of the `vdisplay` cursor share (spec 0020): waits on the ready event, copies the cursor out
//! under the seqlock (`cursor_header::read_snapshot`).

use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use windows::Win32::Foundation::{CloseHandle, ERROR_FILE_NOT_FOUND, HANDLE};
use windows::Win32::System::Memory::{
    FILE_MAP_READ, MEMORY_MAPPED_VIEW_ADDRESS, MapViewOfFile, OpenFileMappingW, UnmapViewOfFile,
};
use windows::Win32::System::Threading::WaitForSingleObject;
use windows::core::HSTRING;

use super::section::open_event;
use super::win32;
use crate::CaptureError;
use crate::cursor_header::{
    CursorPos, CursorView, HEADER_LEN, SECTION_LEN, SHAPE_MAX_BYTES, ShapeInfo, Snapshot,
    cursor_event_name, cursor_section_name, read_snapshot,
};
use crate::header::CONNECTOR_COUNT;

/// Seqlock retries per wake before the wake is skipped (the next update brings a new event).
const READ_TRIES: usize = 8;

/// One cursor change; `shape` borrows the source's buffer until the next `next`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CursorUpdate<'a> {
    pub pos: CursorPos,
    pub shape: Option<(ShapeInfo, &'a [u8])>,
}

/// The cursor of one virtual monitor. One owner thread.
#[derive(Debug)]
pub struct CursorSource {
    view: MappedCursor,
    ready: HANDLE,
    shape: Box<[u8]>,
    last_shape: u32,
    /// The first `next` reads without waiting, so the current cursor goes out at once.
    primed: bool,
}

// SAFETY: the view and handles are process-wide kernel objects; shared fields are read with atomics.
unsafe impl Send for CursorSource {}

impl CursorSource {
    pub fn open(connector: u32) -> Result<Self, CaptureError> {
        if connector >= CONNECTOR_COUNT {
            return Err(CaptureError::NoFrameShare { connector });
        }
        let view = MappedCursor::open(connector)?;
        let ready = open_event(&cursor_event_name(connector))?;
        Ok(Self {
            view,
            ready,
            shape: vec![0u8; SHAPE_MAX_BYTES].into_boxed_slice(),
            last_shape: 0,
            primed: false,
        })
    }

    /// Waits up to `timeout` for a cursor update. `None`: nothing new, no swapchain, or only torn reads.
    // HOT PATH
    pub fn next(&mut self, timeout: Duration) -> Option<CursorUpdate<'_>> {
        if self.primed {
            let ms = u32::try_from(timeout.as_millis()).unwrap_or(u32::MAX);
            // SAFETY: event handle owned by `self`. Timeout or failure: read once anyway (cheap).
            let _ = unsafe { WaitForSingleObject(self.ready, ms) };
        }
        self.primed = true;
        for _ in 0..READ_TRIES {
            match read_snapshot(&self.view, &mut self.shape, self.last_shape) {
                Snapshot::Torn => continue,
                Snapshot::Gone => return None,
                Snapshot::Cursor { pos, shape } => {
                    let shape = shape.map(|info| {
                        self.last_shape = pos.shape_id;
                        (info, &self.shape[..info.len])
                    });
                    return Some(CursorUpdate { pos, shape });
                }
            }
        }
        None
    }

    /// Forgets the last shape, so the next update carries it again (a client asked for it).
    pub fn resend_shape(&mut self) {
        self.last_shape = 0;
    }
}

impl Drop for CursorSource {
    fn drop(&mut self) {
        // SAFETY: opened by `open`, closed once.
        let _ = unsafe { CloseHandle(self.ready) };
    }
}

/// Read-only view of `Global\MixLinkCursor-<n>`.
#[derive(Debug)]
struct MappedCursor {
    handle: HANDLE,
    view: MEMORY_MAPPED_VIEW_ADDRESS,
}

impl MappedCursor {
    fn open(connector: u32) -> Result<Self, CaptureError> {
        // SAFETY: plain Win32 call with an owned name.
        let handle = unsafe {
            OpenFileMappingW(
                FILE_MAP_READ.0,
                false,
                &HSTRING::from(cursor_section_name(connector)),
            )
        }
        .map_err(|e| {
            if e.code() == ERROR_FILE_NOT_FOUND.to_hresult() {
                CaptureError::NoFrameShare { connector }
            } else {
                win32("OpenFileMappingW")(e)
            }
        })?;
        // SAFETY: the driver's section is `SECTION_LEN` bytes.
        let view = unsafe { MapViewOfFile(handle, FILE_MAP_READ, 0, 0, SECTION_LEN) };
        if view.Value.is_null() {
            let error = windows::core::Error::from_thread();
            // SAFETY: opened above, closed once.
            let _ = unsafe { CloseHandle(handle) };
            return Err(win32("MapViewOfFile")(error));
        }
        Ok(Self { handle, view })
    }
}

impl CursorView for MappedCursor {
    fn word(&self, offset: usize) -> u32 {
        // SAFETY: `offset` is a 4-aligned header field inside the mapped view, which lives as long as
        // `self`; the driver writes the seq with Interlocked* and the rest between its two increments.
        let field = unsafe { AtomicU32::from_ptr(self.view.Value.cast::<u8>().add(offset).cast()) };
        field.load(Ordering::Acquire)
    }

    fn copy_pixels(&self, out: &mut [u8]) {
        let len = out.len().min(SHAPE_MAX_BYTES);
        // SAFETY: the pixel area is `SHAPE_MAX_BYTES` long after the header; a copy torn by a concurrent
        // write is discarded by the seq check that follows in `read_snapshot`.
        unsafe {
            std::ptr::copy_nonoverlapping(
                self.view.Value.cast::<u8>().add(HEADER_LEN),
                out.as_mut_ptr(),
                len,
            );
        }
    }
}

impl Drop for MappedCursor {
    fn drop(&mut self) {
        // SAFETY: mapped and opened by `open`, released once.
        unsafe {
            let _ = UnmapViewOfFile(self.view);
            let _ = CloseHandle(self.handle);
        }
    }
}
