//! D3D11 device + DXGI flip-model swapchain bound to one HWND.

use std::num::NonZeroIsize;

use windows::Win32::Foundation::{HMODULE, HWND};
use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP};
use windows::Win32::Graphics::Direct3D11::{
    D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_CREATE_DEVICE_DEBUG, D3D11_CREATE_DEVICE_FLAG,
    D3D11_SDK_VERSION, D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext,
    ID3D11RenderTargetView, ID3D11Texture2D,
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

use crate::RenderError;

const BUFFER_COUNT: u32 = 2;
const FORMAT: DXGI_FORMAT = DXGI_FORMAT_B8G8R8A8_UNORM;

pub(crate) struct D3d11Swapchain {
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    swapchain: IDXGISwapChain1,
    rtv: Option<ID3D11RenderTargetView>,
    minimized: bool,
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
        let mut this = Self {
            device,
            context,
            swapchain,
            rtv: None,
            minimized,
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
        Ok(())
    }

    pub(crate) fn clear_and_present(&mut self, rgba: [f32; 4]) -> Result<(), RenderError> {
        if self.minimized {
            return Ok(());
        }
        let rtv = match &self.rtv {
            Some(rtv) => rtv.clone(),
            None => {
                let rtv = self.create_rtv()?;
                self.rtv = Some(rtv.clone());
                rtv
            }
        };
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

/// Hardware first, WARP fallback (CI, remote sessions). Debug builds ask for the debug layer
/// and retry without it when the Graphics Tools feature is missing.
fn create_device() -> Result<(ID3D11Device, ID3D11DeviceContext), RenderError> {
    let base = D3D11_CREATE_DEVICE_BGRA_SUPPORT;
    let with_debug = D3D11_CREATE_DEVICE_FLAG(base.0 | D3D11_CREATE_DEVICE_DEBUG.0);
    let flag_sets: &[D3D11_CREATE_DEVICE_FLAG] = if cfg!(debug_assertions) {
        &[with_debug, base]
    } else {
        &[base]
    };
    let mut last_error = windows::core::Error::empty();
    for driver in [D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP] {
        for &flags in flag_sets {
            let mut device = None;
            let mut context = None;
            // SAFETY: out-pointers are valid locals; no adapter or software module is passed.
            let result = unsafe {
                D3D11CreateDevice(
                    None,
                    driver,
                    HMODULE::default(),
                    flags,
                    None,
                    D3D11_SDK_VERSION,
                    Some(&mut device),
                    None,
                    Some(&mut context),
                )
            };
            match (result, device, context) {
                (Ok(()), Some(device), Some(context)) => return Ok((device, context)),
                (Err(error), _, _) => last_error = error,
                (Ok(()), _, _) => {}
            }
        }
    }
    Err(RenderError::Win32 {
        context: "D3D11CreateDevice",
        source: last_error,
    })
}

#[cfg(test)]
#[path = "d3d11_tests.rs"]
mod tests;
