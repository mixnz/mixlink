//! D3D11 device on the driver's render adapter and the resources the driver shares (spec 0019).

use windows::Win32::Foundation::{
    CloseHandle, DUPLICATE_SAME_ACCESS, DuplicateHandle, HANDLE, HMODULE, LUID,
};
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_UNKNOWN;
use windows::Win32::Graphics::Direct3D11::{
    D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_CREATE_DEVICE_VIDEO_SUPPORT, D3D11_SDK_VERSION,
    D3D11CreateDevice, ID3D11Device, ID3D11Device5, ID3D11Fence, ID3D11Multithread,
    ID3D11Texture2D,
};
use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIAdapter1, IDXGIFactory4};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcess, PROCESS_DUP_HANDLE};
use windows::core::Interface;

use super::win32;
use crate::CaptureError;

/// Device on the adapter the driver renders on (no cross-adapter copy), video support for the
/// encoder, multithread protected (FFmpeg uses it too).
pub(crate) fn device_on_luid(luid: LUID) -> Result<ID3D11Device5, CaptureError> {
    // SAFETY: plain factory creation and lookup by LUID.
    let adapter: IDXGIAdapter1 = unsafe {
        let factory: IDXGIFactory4 = CreateDXGIFactory1().map_err(win32("CreateDXGIFactory1"))?;
        factory
            .EnumAdapterByLuid(luid)
            .map_err(win32("EnumAdapterByLuid"))?
    };
    let mut device: Option<ID3D11Device> = None;
    // SAFETY: valid out-pointer; the adapter is alive for the call.
    unsafe {
        D3D11CreateDevice(
            &adapter,
            D3D_DRIVER_TYPE_UNKNOWN,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT | D3D11_CREATE_DEVICE_VIDEO_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            None,
        )
    }
    .map_err(win32("D3D11CreateDevice"))?;
    let device = device.ok_or_else(|| win32("D3D11CreateDevice")(windows::core::Error::empty()))?;
    if let Ok(multithread) = device.cast::<ID3D11Multithread>() {
        // SAFETY: live device; returns the previous state only.
        let _ = unsafe { multithread.SetMultithreadProtected(true) };
    }
    device.cast().map_err(win32("ID3D11Device5"))
}

/// The driver's handles live in WUDFHost; this process duplicates them (SYSTEM / Admins may).
#[derive(Debug)]
pub(crate) struct DriverProcess(HANDLE);

impl DriverProcess {
    pub(crate) fn open(pid: u32) -> Result<Self, CaptureError> {
        // SAFETY: plain Win32 call; the handle is closed in Drop.
        unsafe { OpenProcess(PROCESS_DUP_HANDLE, false, pid) }
            .map(Self)
            .map_err(win32("OpenProcess(WUDFHost)"))
    }

    /// A local duplicate of handle `value` inside WUDFHost; the caller closes it.
    fn dup(&self, value: u64) -> Result<HANDLE, CaptureError> {
        let mut out = HANDLE::default();
        // SAFETY: `self.0` is open with PROCESS_DUP_HANDLE; `value` is a handle in that process.
        unsafe {
            DuplicateHandle(
                self.0,
                HANDLE(value as *mut core::ffi::c_void),
                GetCurrentProcess(),
                &mut out,
                0,
                false,
                DUPLICATE_SAME_ACCESS,
            )
        }
        .map_err(win32("DuplicateHandle"))?;
        Ok(out)
    }

    /// A shared texture (IddCx surface or ring slot). Once per handle, never per frame.
    pub(crate) fn open_texture(
        &self,
        device: &ID3D11Device5,
        value: u64,
    ) -> Result<ID3D11Texture2D, CaptureError> {
        let handle = self.dup(value)?;
        // SAFETY: the duplicated handle is closed right after; the texture holds its own reference.
        let texture = unsafe { device.OpenSharedResource1::<ID3D11Texture2D>(handle) };
        // SAFETY: duplicated above, closed once.
        let _ = unsafe { CloseHandle(handle) };
        texture.map_err(win32("OpenSharedResource1"))
    }

    pub(crate) fn open_fence(
        &self,
        device: &ID3D11Device5,
        value: u64,
    ) -> Result<ID3D11Fence, CaptureError> {
        let handle = self.dup(value)?;
        let mut fence: Option<ID3D11Fence> = None;
        // SAFETY: valid out-pointer; the duplicated handle is closed right after.
        let opened = unsafe { device.OpenSharedFence(handle, &mut fence) };
        // SAFETY: duplicated above, closed once.
        let _ = unsafe { CloseHandle(handle) };
        opened.map_err(win32("OpenSharedFence"))?;
        fence.ok_or_else(|| win32("OpenSharedFence")(windows::core::Error::empty()))
    }
}

impl Drop for DriverProcess {
    fn drop(&mut self) {
        // SAFETY: opened in `open`, closed once.
        let _ = unsafe { CloseHandle(self.0) };
    }
}
