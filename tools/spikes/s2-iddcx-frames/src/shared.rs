//! Receiver side of the driver's FrameShare: the shared-memory header and the texture ring + fence.

use std::mem::size_of;
use std::ptr::{addr_of, read_volatile};

use windows::Win32::Foundation::{
    CloseHandle, DUPLICATE_SAME_ACCESS, DuplicateHandle, HANDLE, LUID,
};
use windows::Win32::Graphics::Direct3D11::{
    ID3D11Device5, ID3D11DeviceContext4, ID3D11Fence, ID3D11Texture2D,
};
use windows::Win32::System::Memory::{
    FILE_MAP_READ, MEMORY_MAPPED_VIEW_ADDRESS, MapViewOfFile, OpenFileMappingW, UnmapViewOfFile,
};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcess, PROCESS_DUP_HANDLE};
use windows::core::HSTRING;

use crate::d3d::device_on_luid;
use crate::header::{S2Header, S2Slot, SECTION_NAME, SLOT_COUNT};
use crate::util::BoxResult;

/// Read-only view of `Global\MixLinkS2Frames`.
pub struct Section {
    handle: HANDLE,
    view: MEMORY_MAPPED_VIEW_ADDRESS,
}

impl Section {
    pub fn open() -> BoxResult<Self> {
        // SAFETY: plain Win32 calls; the view size matches the driver's header.
        unsafe {
            let handle = OpenFileMappingW(FILE_MAP_READ.0, false, &HSTRING::from(SECTION_NAME))?;
            let view = MapViewOfFile(handle, FILE_MAP_READ, 0, 0, size_of::<S2Header>());
            if view.Value.is_null() {
                let _ = CloseHandle(handle);
                return Err("MapViewOfFile failed".into());
            }
            Ok(Self { handle, view })
        }
    }

    fn ptr(&self) -> *const S2Header {
        self.view.Value.cast()
    }

    pub fn header(&self) -> S2Header {
        // SAFETY: the view is mapped for the lifetime of `self` and holds one S2Header.
        unsafe { read_volatile(self.ptr()) }
    }

    // HOT PATH
    pub fn latest_seq(&self) -> u64 {
        // SAFETY: as in `header`.
        unsafe { read_volatile(addr_of!((*self.ptr()).latest_seq)) }
    }

    // HOT PATH
    pub fn slot(&self, index: usize) -> S2Slot {
        // SAFETY: as in `header`; `index` < SLOT_COUNT (callers use seq % SLOT_COUNT).
        unsafe { read_volatile(addr_of!((*self.ptr()).slots[index % SLOT_COUNT])) }
    }

    // HOT PATH
    pub fn slot_seq(&self, index: usize) -> u64 {
        // SAFETY: as in `slot`.
        unsafe { read_volatile(addr_of!((*self.ptr()).slots[index % SLOT_COUNT].seq)) }
    }
}

impl Drop for Section {
    fn drop(&mut self) {
        // SAFETY: both were created in `open` and are released once.
        unsafe {
            let _ = UnmapViewOfFile(self.view);
            let _ = CloseHandle(self.handle);
        }
    }
}

/// The ring and fence opened on a device created on the driver's render adapter.
pub struct SharedFrames {
    pub device: ID3D11Device5,
    pub ctx: ID3D11DeviceContext4,
    pub ring: Vec<ID3D11Texture2D>,
    pub fence: ID3D11Fence,
    pub luid: LUID,
}

/// Duplicates one handle out of WUDFHost (needs PROCESS_DUP_HANDLE on it: SYSTEM has it).
fn dup(process: HANDLE, value: u64) -> BoxResult<HANDLE> {
    let mut out = HANDLE::default();
    // SAFETY: `process` is open with PROCESS_DUP_HANDLE; `value` is a handle in that process.
    unsafe {
        DuplicateHandle(
            process,
            HANDLE(value as *mut core::ffi::c_void),
            GetCurrentProcess(),
            &mut out,
            0,
            false,
            DUPLICATE_SAME_ACCESS,
        )?
    };
    Ok(out)
}

impl SharedFrames {
    pub fn open(h: &S2Header) -> BoxResult<Self> {
        let luid = LUID {
            LowPart: h.luid_low,
            HighPart: h.luid_high,
        };
        let (device, ctx) = device_on_luid(luid)?;
        // SAFETY: handles are closed right after the resources are opened (they hold their own refs).
        unsafe {
            let process = OpenProcess(PROCESS_DUP_HANDLE, false, h.wudf_pid)?;
            let mut ring = Vec::with_capacity(SLOT_COUNT);
            let mut result = Ok(());
            for &value in &h.texture_handles {
                match dup(process, value) {
                    Ok(handle) => {
                        let tex = device.OpenSharedResource1::<ID3D11Texture2D>(handle);
                        let _ = CloseHandle(handle);
                        match tex {
                            Ok(t) => ring.push(t),
                            Err(e) => result = Err(e.into()),
                        }
                    }
                    Err(e) => result = Err(e),
                }
            }
            let fence = dup(process, h.fence_handle).and_then(|handle| {
                let mut fence: Option<ID3D11Fence> = None;
                let opened = device.OpenSharedFence(handle, &mut fence);
                let _ = CloseHandle(handle);
                opened?;
                fence.ok_or_else(|| "OpenSharedFence returned nothing".into())
            });
            let _ = CloseHandle(process);
            result?;
            Ok(Self {
                device,
                ctx,
                ring,
                fence: fence?,
                luid,
            })
        }
    }
}
