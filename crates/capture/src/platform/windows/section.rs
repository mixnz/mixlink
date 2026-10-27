//! The mapped frame header of one connector and its two events (spec 0019).

use std::mem::size_of;
use std::ptr::read_volatile;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use windows::Win32::Foundation::{CloseHandle, ERROR_FILE_NOT_FOUND, HANDLE};
use windows::Win32::System::Memory::{
    FILE_MAP_READ, FILE_MAP_WRITE, MEMORY_MAPPED_VIEW_ADDRESS, MapViewOfFile, OpenFileMappingW,
    UnmapViewOfFile,
};
use windows::Win32::System::Threading::{
    EVENT_MODIFY_STATE, OpenEventW, SYNCHRONIZATION_SYNCHRONIZE, SetEvent,
};
use windows::core::HSTRING;

use super::win32;
use crate::CaptureError;
use crate::header::{
    CONNECTOR_COUNT, FrameHeader, FrameSlot, SLOT_COUNT, ready_event_name, released_event_name,
    section_name,
};
use crate::reader::HeaderView;

/// Read-write view of `Global\MixLinkFrames-<n>` (the reader writes its own fields).
#[derive(Debug)]
pub(crate) struct Section {
    handle: HANDLE,
    view: MEMORY_MAPPED_VIEW_ADDRESS,
    ready: HANDLE,
    released: HANDLE,
}

// SAFETY: the view and handles are process-wide kernel objects; every access to shared fields goes
// through atomics or volatile reads.
unsafe impl Send for Section {}

fn close_all(handles: &[HANDLE]) {
    for &handle in handles {
        if !handle.is_invalid() {
            // SAFETY: each handle was opened by `Section::open` and is closed once.
            let _ = unsafe { CloseHandle(handle) };
        }
    }
}

impl Section {
    pub(crate) fn open(connector: u32) -> Result<Self, CaptureError> {
        if connector >= CONNECTOR_COUNT {
            return Err(CaptureError::NoFrameShare { connector });
        }
        let access = FILE_MAP_READ.0 | FILE_MAP_WRITE.0;
        // SAFETY: plain Win32 call with an owned name.
        let handle =
            unsafe { OpenFileMappingW(access, false, &HSTRING::from(section_name(connector))) }
                .map_err(|e| {
                    if e.code() == ERROR_FILE_NOT_FOUND.to_hresult() {
                        CaptureError::NoFrameShare { connector }
                    } else {
                        CaptureError::Win32 {
                            context: "OpenFileMappingW",
                            source: e,
                        }
                    }
                })?;
        let events = open_event(&ready_event_name(connector)).and_then(|ready| {
            match open_event(&released_event_name(connector)) {
                Ok(released) => Ok((ready, released)),
                Err(e) => {
                    close_all(&[ready]);
                    Err(e)
                }
            }
        });
        let (ready, released) = match events {
            Ok(pair) => pair,
            Err(e) => {
                close_all(&[handle]);
                return Err(e);
            }
        };
        // SAFETY: the view size matches the driver's section (`FrameHeader`).
        let view = unsafe {
            MapViewOfFile(
                handle,
                windows::Win32::System::Memory::FILE_MAP(access),
                0,
                0,
                size_of::<FrameHeader>(),
            )
        };
        if view.Value.is_null() {
            close_all(&[handle, ready, released]);
            return Err(win32("MapViewOfFile")(windows::core::Error::from_thread()));
        }
        Ok(Self {
            handle,
            view,
            ready,
            released,
        })
    }

    fn ptr(&self) -> *mut FrameHeader {
        self.view.Value.cast()
    }

    pub(crate) fn ready_event(&self) -> HANDLE {
        self.ready
    }

    /// Snapshot of every field (not hot: once per generation).
    pub(crate) fn header(&self) -> FrameHeader {
        // SAFETY: the view is mapped for the lifetime of `self` and holds one FrameHeader.
        unsafe { read_volatile(self.ptr()) }
    }

    fn u64_at(&self, field: *mut u64) -> &AtomicU64 {
        // SAFETY: `field` points into the mapped header (8-byte aligned, `repr(C)` mirror of the
        // driver's pack(8) struct) and lives as long as `self`; the driver writes it with Interlocked*.
        unsafe { AtomicU64::from_ptr(field) }
    }

    fn u32_at(&self, field: *mut u32) -> &AtomicU32 {
        // SAFETY: as in `u64_at` (4-byte aligned).
        unsafe { AtomicU32::from_ptr(field) }
    }

    // HOT PATH
    pub(crate) fn bump_heartbeat(&self) {
        // SAFETY: field projection inside the mapped header, no reference to the whole struct.
        let field = unsafe { &raw mut (*self.ptr()).reader_seq };
        self.u64_at(field).fetch_add(1, Ordering::Release);
    }

    pub(crate) fn set_reader_pid(&self, pid: u32) {
        // SAFETY: as in `bump_heartbeat`.
        let field = unsafe { &raw mut (*self.ptr()).reader_pid };
        self.u32_at(field).store(pid, Ordering::Release);
    }

    // HOT PATH
    /// Tells the driver the reader is done with `seq` (the surface may be reused).
    pub(crate) fn release(&self, seq: u64) {
        // SAFETY: as in `bump_heartbeat`.
        let field = unsafe { &raw mut (*self.ptr()).released_seq };
        self.u64_at(field).store(seq, Ordering::Release);
        // SAFETY: event handle owned by `self`. A failure only delays the driver to its cap.
        let _ = unsafe { SetEvent(self.released) };
    }

    pub(crate) fn hold_timeouts(&self) -> u32 {
        // SAFETY: as in `bump_heartbeat`.
        let field = unsafe { &raw mut (*self.ptr()).hold_timeouts };
        self.u32_at(field).load(Ordering::Acquire)
    }

    pub(crate) fn ring_fallbacks(&self) -> u32 {
        // SAFETY: as in `bump_heartbeat`.
        let field = unsafe { &raw mut (*self.ptr()).ring_fallbacks };
        self.u32_at(field).load(Ordering::Acquire)
    }
}

impl HeaderView for Section {
    fn magic(&self) -> u32 {
        // SAFETY: as in `bump_heartbeat`.
        let field = unsafe { &raw mut (*self.ptr()).magic };
        self.u32_at(field).load(Ordering::Acquire)
    }

    fn version(&self) -> u32 {
        // SAFETY: as in `header`; written before Magic.
        unsafe { read_volatile(&raw const (*self.ptr()).version) }
    }

    fn generation(&self) -> u32 {
        // SAFETY: as in `version`.
        unsafe { read_volatile(&raw const (*self.ptr()).generation) }
    }

    fn size(&self) -> (u32, u32) {
        // SAFETY: as in `version`.
        unsafe {
            (
                read_volatile(&raw const (*self.ptr()).width),
                read_volatile(&raw const (*self.ptr()).height),
            )
        }
    }

    fn latest_seq(&self) -> u64 {
        // SAFETY: as in `bump_heartbeat`.
        let field = unsafe { &raw mut (*self.ptr()).latest_seq };
        self.u64_at(field).load(Ordering::Acquire)
    }

    fn slot(&self, index: usize) -> FrameSlot {
        // SAFETY: as in `header`; the index is reduced modulo SLOT_COUNT.
        unsafe { read_volatile(&raw const (*self.ptr()).slots[index % SLOT_COUNT]) }
    }

    fn slot_seq(&self, index: usize) -> u64 {
        // SAFETY: as in `slot`.
        let field = unsafe { &raw mut (*self.ptr()).slots[index % SLOT_COUNT].seq };
        self.u64_at(field).load(Ordering::Acquire)
    }
}

impl Drop for Section {
    fn drop(&mut self) {
        // SAFETY: the view was mapped in `open` and is unmapped once.
        let _ = unsafe { UnmapViewOfFile(self.view) };
        close_all(&[self.handle, self.ready, self.released]);
    }
}

fn open_event(name: &str) -> Result<HANDLE, CaptureError> {
    // SAFETY: plain Win32 call with an owned name.
    unsafe {
        OpenEventW(
            SYNCHRONIZATION_SYNCHRONIZE | EVENT_MODIFY_STATE,
            false,
            &HSTRING::from(name),
        )
    }
    .map_err(win32("OpenEventW"))
}
