//! D3D11 device creation (hardware, WARP fallback) and the GPU-idle event query.

use windows::Win32::Foundation::HMODULE;
use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP};
use windows::Win32::Graphics::Direct3D11::{
    D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_CREATE_DEVICE_DEBUG, D3D11_CREATE_DEVICE_FLAG,
    D3D11_CREATE_DEVICE_VIDEO_SUPPORT, D3D11_QUERY_DESC, D3D11_QUERY_EVENT, D3D11_SDK_VERSION,
    D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Multithread, ID3D11Query,
};
use windows::core::Interface;

use crate::RenderError;

fn win32(context: &'static str) -> impl FnOnce(windows::core::Error) -> RenderError {
    move |source| RenderError::Win32 { context, source }
}

pub(crate) fn create_event_query(device: &ID3D11Device) -> Result<ID3D11Query, RenderError> {
    let desc = D3D11_QUERY_DESC {
        Query: D3D11_QUERY_EVENT,
        MiscFlags: 0,
    };
    let mut query = None;
    // SAFETY: `desc` outlives the call; valid out-pointer.
    unsafe { device.CreateQuery(&desc, Some(&mut query)) }.map_err(win32("CreateQuery"))?;
    query.ok_or_else(|| win32("CreateQuery")(windows::core::Error::empty()))
}

/// Hardware first, WARP fallback (CI, remote sessions). Video support (D3D11VA decode on this
/// device) is asked first and dropped where it fails; debug builds ask for the debug layer and
/// retry without it when the Graphics Tools feature is missing. Multithread protection is on:
/// FFmpeg decodes on the same device.
pub(crate) fn create_device() -> Result<(ID3D11Device, ID3D11DeviceContext), RenderError> {
    let (device, context) = create_device_with_flags()?;
    if let Ok(multithread) = device.cast::<ID3D11Multithread>() {
        // SAFETY: live device; returns the previous state only.
        let _ = unsafe { multithread.SetMultithreadProtected(true) };
    }
    Ok((device, context))
}

fn create_device_with_flags() -> Result<(ID3D11Device, ID3D11DeviceContext), RenderError> {
    let flags = |extra: &[D3D11_CREATE_DEVICE_FLAG]| {
        D3D11_CREATE_DEVICE_FLAG(
            extra
                .iter()
                .fold(D3D11_CREATE_DEVICE_BGRA_SUPPORT.0, |acc, f| acc | f.0),
        )
    };
    let (video, debug) = (D3D11_CREATE_DEVICE_VIDEO_SUPPORT, D3D11_CREATE_DEVICE_DEBUG);
    let debug_sets = [
        flags(&[video, debug]),
        flags(&[video]),
        flags(&[debug]),
        flags(&[]),
    ];
    let release_sets = [flags(&[video]), flags(&[])];
    let flag_sets: &[D3D11_CREATE_DEVICE_FLAG] = if cfg!(debug_assertions) {
        &debug_sets
    } else {
        &release_sets
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
