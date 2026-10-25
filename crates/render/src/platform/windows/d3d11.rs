//! D3D11 device + DXGI flip-model swapchain bound to one HWND.

use std::num::NonZeroIsize;

use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Direct3D11::{
    ID3D11Device, ID3D11DeviceContext, ID3D11Query, ID3D11RenderTargetView, ID3D11Texture2D,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_ALPHA_MODE_UNSPECIFIED, DXGI_FORMAT, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_UNKNOWN,
    DXGI_SAMPLE_DESC,
};
use windows::Win32::Graphics::Dxgi::{
    DXGI_MWA_NO_ALT_ENTER, DXGI_PRESENT, DXGI_SCALING_STRETCH, DXGI_SWAP_CHAIN_DESC1,
    DXGI_SWAP_CHAIN_FLAG, DXGI_SWAP_EFFECT_FLIP_DISCARD, DXGI_USAGE_RENDER_TARGET_OUTPUT,
    IDXGIDevice, IDXGIFactory2, IDXGISwapChain1,
};
use windows::core::Interface;

use super::device::{create_device, create_event_query};
use super::nv12::Nv12Pass;
use crate::{RenderError, VideoFrame};

const BUFFER_COUNT: u32 = 2;
const FORMAT: DXGI_FORMAT = DXGI_FORMAT_B8G8R8A8_UNORM;

pub(crate) struct D3d11Swapchain {
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    swapchain: IDXGISwapChain1,
    rtv: Option<ID3D11RenderTargetView>,
    minimized: bool,
    size: (u32, u32),
    nv12: Nv12Pass,
    /// Event query for `wait_gpu` (made once).
    gpu_done: ID3D11Query,
}

fn win32(context: &'static str) -> impl FnOnce(windows::core::Error) -> RenderError {
    move |source| RenderError::Win32 { context, source }
}

impl D3d11Swapchain {
    pub(crate) fn new(hwnd: NonZeroIsize, width: u32, height: u32) -> Result<Self, RenderError> {
        let hwnd = HWND(hwnd.get() as *mut core::ffi::c_void);
        let (device, context) = create_device()?;
        let desc = DXGI_SWAP_CHAIN_DESC1 {
            Width: width.max(1),
            Height: height.max(1),
            Format: FORMAT,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
            BufferCount: BUFFER_COUNT,
            Scaling: DXGI_SCALING_STRETCH,
            SwapEffect: DXGI_SWAP_EFFECT_FLIP_DISCARD,
            AlphaMode: DXGI_ALPHA_MODE_UNSPECIFIED,
            ..Default::default()
        };
        let dxgi_device: IDXGIDevice = device.cast().map_err(win32("IDXGIDevice cast"))?;
        // SAFETY: `device` is a live D3D11 device; `hwnd` belongs to a window the caller keeps
        // alive (`Presenter` owns an `Arc` of it); `desc` outlives the call.
        let swapchain = unsafe {
            let adapter = dxgi_device.GetAdapter().map_err(win32("GetAdapter"))?;
            let factory: IDXGIFactory2 = adapter.GetParent().map_err(win32("GetParent"))?;
            let swapchain = factory
                .CreateSwapChainForHwnd(&device, hwnd, &desc, None, None)
                .map_err(win32("CreateSwapChainForHwnd"))?;
            factory
                .MakeWindowAssociation(hwnd, DXGI_MWA_NO_ALT_ENTER)
                .map_err(win32("MakeWindowAssociation"))?;
            swapchain
        };
        let minimized = width == 0 || height == 0;
        let nv12 = Nv12Pass::new(&device)?;
        let gpu_done = create_event_query(&device)?;
        let mut this = Self {
            device,
            context,
            swapchain,
            rtv: None,
            minimized,
            size: (width, height),
            nv12,
            gpu_done,
        };
        if !minimized {
            this.rtv = Some(this.create_rtv()?);
        }
        Ok(this)
    }

    pub(crate) fn resize(&mut self, width: u32, height: u32) -> Result<(), RenderError> {
        if width == 0 || height == 0 {
            self.minimized = true;
            return Ok(());
        }
        // Every reference to the back buffers must be released before ResizeBuffers.
        self.rtv = None;
        // SAFETY: no views of the swapchain buffers are alive (unbound here, dropped above).
        unsafe {
            self.context.OMSetRenderTargets(None, None);
            self.swapchain
                .ResizeBuffers(
                    0,
                    width,
                    height,
                    DXGI_FORMAT_UNKNOWN,
                    DXGI_SWAP_CHAIN_FLAG(0),
                )
                .map_err(win32("ResizeBuffers"))?;
        }
        self.rtv = Some(self.create_rtv()?);
        self.minimized = false;
        self.size = (width, height);
        Ok(())
    }

    pub(crate) fn device(&self) -> ID3D11Device {
        self.device.clone()
    }

    // HOT PATH
    pub(crate) fn present_nv12(&mut self, frame: &VideoFrame) -> Result<(), RenderError> {
        if self.minimized {
            return Ok(());
        }
        let rtv = self.current_rtv()?;
        self.nv12.draw(&self.context, frame, &rtv, self.size)?;
        // SAFETY: live swapchain; sync interval 0: never waits for vblank, DWM shows the newest.
        unsafe { self.swapchain.Present(0, DXGI_PRESENT(0)) }
            .ok()
            .map_err(win32("Present"))
    }

    /// Blocks until the GPU finished all submitted work (measuring only; spins).
    pub(crate) fn wait_gpu(&mut self) {
        let mut done = windows::core::BOOL(0);
        // SAFETY: live query and context; `done` is a BOOL-sized out-buffer. S_FALSE (not ready)
        // leaves it untouched; an error (device removed) ends the wait.
        unsafe {
            self.context.End(&self.gpu_done);
            self.context.Flush();
            while !done.as_bool() {
                let size = std::mem::size_of::<windows::core::BOOL>() as u32;
                let out = (&mut done as *mut windows::core::BOOL).cast();
                if self
                    .context
                    .GetData(&self.gpu_done, Some(out), size, 0)
                    .is_err()
                {
                    return;
                }
                std::hint::spin_loop();
            }
        }
    }

    fn current_rtv(&mut self) -> Result<ID3D11RenderTargetView, RenderError> {
        if let Some(rtv) = &self.rtv {
            return Ok(rtv.clone());
        }
        let rtv = self.create_rtv()?;
        self.rtv = Some(rtv.clone());
        Ok(rtv)
    }

    pub(crate) fn clear_and_present(&mut self, rgba: [f32; 4]) -> Result<(), RenderError> {
        if self.minimized {
            return Ok(());
        }
        let rtv = self.current_rtv()?;
        // SAFETY: `rtv` views buffer 0 of this swapchain and stays alive for the whole block.
        unsafe {
            self.context
                .OMSetRenderTargets(Some(&[Some(rtv.clone())]), None);
            self.context.ClearRenderTargetView(&rtv, &rgba);
            // Success codes such as DXGI_STATUS_OCCLUDED are not errors.
            self.swapchain
                .Present(1, DXGI_PRESENT(0))
                .ok()
                .map_err(win32("Present"))
        }
    }

    fn create_rtv(&self) -> Result<ID3D11RenderTargetView, RenderError> {
        let mut rtv = None;
        // SAFETY: buffer 0 always exists on a flip-model swapchain; `rtv` is a valid out-pointer.
        unsafe {
            let buffer: ID3D11Texture2D =
                self.swapchain.GetBuffer(0).map_err(win32("GetBuffer"))?;
            self.device
                .CreateRenderTargetView(&buffer, None, Some(&mut rtv))
                .map_err(win32("CreateRenderTargetView"))?;
        }
        rtv.ok_or_else(|| win32("CreateRenderTargetView")(windows::core::Error::empty()))
    }
}

#[cfg(test)]
#[path = "d3d11_tests.rs"]
mod tests;
