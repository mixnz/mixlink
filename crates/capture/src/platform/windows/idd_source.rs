//! `IddSource`: frames of one MixLink virtual monitor from the `vdisplay` frame share (spec 0019).

use std::time::{Duration, Instant};

use mixlink_core::clock::{self, Ticks};
use windows::Win32::Foundation::{CloseHandle, HANDLE, LUID};
use windows::Win32::Graphics::Direct3D11::{
    ID3D11Device, ID3D11Device5, ID3D11Fence, ID3D11Texture2D,
};
use windows::Win32::System::Threading::{CreateEventW, WaitForSingleObject};
use windows::core::Interface;

use super::device::{DriverProcess, device_on_luid};
use super::section::Section;
use super::win32;
use crate::CaptureError;
use crate::header::{MAGIC, SLOT_COUNT, VERSION, slot_index};
use crate::reader::{HeaderView, Poll, ReaderState};
use crate::source::{CaptureEvent, Frame, FrameSource};
use crate::surface_cache::SurfaceCache;

/// No shared swapchain for this long → `CaptureError::NoSwapchain`.
const GONE_LIMIT: Duration = Duration::from_secs(2);
/// Upper bound of the CPU wait for the driver's fence (DWM's rendering is normally done already).
const FENCE_WAIT_MS: u32 = 100;

/// Everything opened for one generation of the frame share.
#[derive(Debug)]
struct Generation {
    width: u32,
    height: u32,
    process: DriverProcess,
    fence: ID3D11Fence,
    ring: [Option<ID3D11Texture2D>; SLOT_COUNT],
    surfaces: SurfaceCache<ID3D11Texture2D>,
}

/// Reader of one connector's frame share. Not `Sync`: one thread waits and encodes.
#[derive(Debug)]
pub struct IddSource {
    section: Section,
    state: ReaderState,
    device: Option<(LUID, ID3D11Device5)>,
    generation: Option<Generation>,
    fence_event: HANDLE,
    gone_since: Option<Instant>,
}

impl IddSource {
    /// Opens the frame share of monitor `connector` (from the driver's monitor query) and marks
    /// this process as its reader. Needs SYSTEM or an elevated Administrator.
    pub fn open(connector: u32) -> Result<Self, CaptureError> {
        let section = Section::open(connector)?;
        // SAFETY: auto-reset unnamed event, closed in Drop.
        let fence_event =
            unsafe { CreateEventW(None, false, false, None) }.map_err(win32("CreateEventW"))?;
        section.set_reader_pid(std::process::id());
        section.bump_heartbeat();
        Ok(Self {
            section,
            state: ReaderState::new(),
            device: None,
            generation: None,
            fence_event,
            gone_since: None,
        })
    }

    /// The device frames are opened on (the driver's render adapter); `None` before the first
    /// `Reopened`. The encoder must use this device.
    pub fn device(&self) -> Option<ID3D11Device> {
        self.device.as_ref().and_then(|(_, d)| d.cast().ok())
    }

    pub fn hold_timeouts(&self) -> u32 {
        self.section.hold_timeouts()
    }

    pub fn ring_fallbacks(&self) -> u32 {
        self.section.ring_fallbacks()
    }

    fn reopen(&mut self) -> Result<(u32, u32), CaptureError> {
        self.generation = None;
        let h = self.section.header();
        let luid = LUID {
            LowPart: h.luid_low,
            HighPart: h.luid_high,
        };
        let device = match &self.device {
            Some((current, device))
                if current.LowPart == luid.LowPart && current.HighPart == luid.HighPart =>
            {
                device.clone()
            }
            _ => {
                let device = device_on_luid(luid)?;
                self.device = Some((luid, device.clone()));
                device
            }
        };
        let process = DriverProcess::open(h.wudf_pid)?;
        let fence = process.open_fence(&device, h.fence_handle)?;
        let mut ring: [Option<ID3D11Texture2D>; SLOT_COUNT] = Default::default();
        for (slot, &value) in ring.iter_mut().zip(h.texture_handles.iter()) {
            if value != 0 {
                *slot = Some(process.open_texture(&device, value)?);
            }
        }
        self.generation = Some(Generation {
            width: h.width,
            height: h.height,
            process,
            fence,
            ring,
            surfaces: SurfaceCache::new(),
        });
        Ok((h.width, h.height))
    }

    fn gone(&mut self) -> Result<(), CaptureError> {
        if self.section.magic() == MAGIC && self.section.version() != VERSION {
            return Err(CaptureError::WrongVersion {
                found: self.section.version(),
            });
        }
        let since = *self.gone_since.get_or_insert_with(Instant::now);
        if since.elapsed() > GONE_LIMIT {
            return Err(CaptureError::NoSwapchain);
        }
        Ok(())
    }
}

impl FrameSource for IddSource {
    type Frame<'a> = CapturedFrame<'a>;

    // HOT PATH
    fn next(
        &mut self,
        timeout: Duration,
    ) -> Result<Option<CaptureEvent<CapturedFrame<'_>>>, CaptureError> {
        self.section.bump_heartbeat();
        let mut poll = self.state.poll(&self.section);
        if matches!(poll, Poll::Idle | Poll::Gone) {
            let ms = u32::try_from(timeout.as_millis()).unwrap_or(u32::MAX);
            // SAFETY: event handle owned by the section. Timeout or failure: poll once more anyway.
            let _ = unsafe { WaitForSingleObject(self.section.ready_event(), ms) };
            self.section.bump_heartbeat();
            poll = self.state.poll(&self.section);
        }
        let (seq, slot) = match poll {
            Poll::Idle => return Ok(None),
            Poll::Gone => return self.gone().map(|()| None),
            Poll::Reopen { .. } => {
                self.gone_since = None;
                let (width, height) = self.reopen()?;
                return Ok(Some(CaptureEvent::Reopened { width, height }));
            }
            Poll::Frame { seq, slot } => (seq, slot),
        };
        self.gone_since = None;
        let (Some(generation), Some((_, device))) =
            (self.generation.as_mut(), self.device.as_ref())
        else {
            self.section.release(seq);
            return Ok(None);
        };
        let texture = if slot.surface_handle != 0 {
            let process = &generation.process;
            match generation
                .surfaces
                .get_or_open(slot.surface_handle, |value| {
                    process.open_texture(device, value)
                }) {
                Ok(texture) => texture,
                Err(e) => {
                    self.section.release(seq);
                    return Err(e);
                }
            }
        } else {
            match &generation.ring[slot_index(seq)] {
                Some(texture) => texture,
                None => {
                    self.section.release(seq);
                    return Ok(None);
                }
            }
        };
        // SAFETY: fence and event are alive; a timeout only means DWM's rendering was late.
        unsafe {
            if generation.fence.GetCompletedValue() < seq
                && generation
                    .fence
                    .SetEventOnCompletion(seq, self.fence_event)
                    .is_ok()
            {
                let _ = WaitForSingleObject(self.fence_event, FENCE_WAIT_MS);
            }
        }
        Ok(Some(CaptureEvent::Frame(CapturedFrame {
            texture,
            section: &self.section,
            width: generation.width,
            height: generation.height,
            seq,
            present_qpc: Ticks::from_raw(slot.present_qpc),
            acquire_qpc: Ticks::from_raw(slot.acquire_qpc),
            captured: clock::now(),
        })))
    }
}

impl Drop for IddSource {
    fn drop(&mut self) {
        self.section.set_reader_pid(0);
        // SAFETY: created in `open`, closed once.
        let _ = unsafe { CloseHandle(self.fence_event) };
    }
}

/// A frame on the reader's device. Valid until dropped; dropping it releases the surface to the driver.
#[derive(Debug)]
pub struct CapturedFrame<'a> {
    texture: &'a ID3D11Texture2D,
    section: &'a Section,
    width: u32,
    height: u32,
    seq: u64,
    present_qpc: Ticks,
    acquire_qpc: Ticks,
    captured: Ticks,
}

impl CapturedFrame<'_> {
    /// BGRA texture on `IddSource::device()`; read it on the GPU only, before dropping the frame.
    pub fn texture(&self) -> &ID3D11Texture2D {
        self.texture
    }
}

impl Frame for CapturedFrame<'_> {
    fn seq(&self) -> u64 {
        self.seq
    }
    fn width(&self) -> u32 {
        self.width
    }
    fn height(&self) -> u32 {
        self.height
    }
    fn present_qpc(&self) -> Ticks {
        self.present_qpc
    }
    fn acquire_qpc(&self) -> Ticks {
        self.acquire_qpc
    }
    fn captured(&self) -> Ticks {
        self.captured
    }
}

impl Drop for CapturedFrame<'_> {
    // HOT PATH
    fn drop(&mut self) {
        self.section.release(self.seq);
    }
}
