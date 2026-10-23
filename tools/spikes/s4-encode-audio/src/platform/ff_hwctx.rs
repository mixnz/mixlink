//! FFmpeg hardware device and frame-pool contexts for the encoder (D3D11 ours or FFmpeg-created, QSV derived).

use std::ffi::{CString, c_void};
use std::ptr;

use ffmpeg_sys_next as ff;
use windows::Win32::Graphics::Direct3D11::ID3D11Device;
use windows::core::Interface;

use crate::args::{EncodeArgs, Format};
use crate::platform::ff_util::check;
use crate::util::BoxResult;

/// Fixed pool size (QSV derives one surface per pool texture).
pub const POOL_SIZE: i32 = 8;
const D3D11VA: ff::AVHWDeviceType = ff::AVHWDeviceType::AV_HWDEVICE_TYPE_D3D11VA;
const QSV: ff::AVHWDeviceType = ff::AVHWDeviceType::AV_HWDEVICE_TYPE_QSV;

/// Owned device / frames refs (null until opened); released on drop.
pub struct HwRefs {
    pub device: *mut ff::AVBufferRef,
    pub frames: *mut ff::AVBufferRef,
    pub qsv_device: *mut ff::AVBufferRef,
    pub qsv_frames: *mut ff::AVBufferRef,
}

impl Default for HwRefs {
    fn default() -> Self {
        Self {
            device: ptr::null_mut(),
            frames: ptr::null_mut(),
            qsv_device: ptr::null_mut(),
            qsv_frames: ptr::null_mut(),
        }
    }
}

impl HwRefs {
    pub fn release(&mut self) {
        // SAFETY: owned refs (or null).
        unsafe {
            ff::av_buffer_unref(&mut self.qsv_frames);
            ff::av_buffer_unref(&mut self.qsv_device);
            ff::av_buffer_unref(&mut self.frames);
            ff::av_buffer_unref(&mut self.device);
        }
    }

    /// The D3D11 device the textures live on.
    pub fn d3d11_device(&self) -> BoxResult<ID3D11Device> {
        if self.device.is_null() {
            return Err("no d3d11 device context".into());
        }
        // SAFETY: the device ref is alive; its hwctx holds a valid ID3D11Device pointer.
        unsafe {
            let hw = (*self.device).data as *mut ff::AVHWDeviceContext;
            let d3d = (*hw).hwctx as *mut ff::AVD3D11VADeviceContext;
            let raw = (*d3d).device as *mut c_void;
            Ok(ID3D11Device::from_raw_borrowed(&raw)
                .ok_or("no d3d11 device")?
                .clone())
        }
    }

    /// D3D11 device context around our own device.
    pub fn open_d3d11(&mut self, dev: &ID3D11Device) -> BoxResult<()> {
        // SAFETY: FFmpeg owns the device ref; it releases `device` on free, so we hand it an AddRef'd pointer.
        unsafe {
            self.device = ff::av_hwdevice_ctx_alloc(D3D11VA);
            if self.device.is_null() {
                return Err("av_hwdevice_ctx_alloc failed".into());
            }
            let hw = (*self.device).data as *mut ff::AVHWDeviceContext;
            let d3d = (*hw).hwctx as *mut ff::AVD3D11VADeviceContext;
            (*d3d).device = dev.clone().into_raw() as *mut ff::ID3D11Device;
            check(
                ff::av_hwdevice_ctx_init(self.device),
                "av_hwdevice_ctx_init",
            )?;
        }
        Ok(())
    }

    /// D3D11 device created by FFmpeg on adapter `index` (multithread protection on, as QSV wants).
    pub fn open_d3d11_ffmpeg(&mut self, index: u32) -> BoxResult<()> {
        let device = CString::new(index.to_string())?;
        // SAFETY: out-pointer is a field of self; FFmpeg owns what it creates.
        unsafe {
            check(
                ff::av_hwdevice_ctx_create(
                    &mut self.device,
                    D3D11VA,
                    device.as_ptr(),
                    ptr::null_mut(),
                    0,
                ),
                "create d3d11va device",
            )?;
        }
        Ok(())
    }

    /// D3D11 pool of `pool` textures with `bind` flags (`pool` 0 = single textures, grown on demand).
    pub fn open_d3d11_frames(&mut self, a: &EncodeArgs, bind: u32, pool: i32) -> BoxResult<()> {
        // SAFETY: the frames ref comes from our live device ref; fields set before init as documented.
        unsafe {
            self.frames = ff::av_hwframe_ctx_alloc(self.device);
            if self.frames.is_null() {
                return Err("av_hwframe_ctx_alloc failed".into());
            }
            let fc = (*self.frames).data as *mut ff::AVHWFramesContext;
            (*fc).format = ff::AVPixelFormat::AV_PIX_FMT_D3D11;
            (*fc).sw_format = match a.format {
                Format::Bgra => ff::AVPixelFormat::AV_PIX_FMT_BGRA,
                Format::Nv12 => ff::AVPixelFormat::AV_PIX_FMT_NV12,
            };
            (*fc).width = a.width as i32;
            (*fc).height = a.height as i32;
            (*fc).initial_pool_size = pool;
            let d3d = (*fc).hwctx as *mut ff::AVD3D11VAFramesContext;
            (*d3d).BindFlags = bind;
            check(ff::av_hwframe_ctx_init(self.frames), "av_hwframe_ctx_init")?;
        }
        Ok(())
    }

    /// QSV device + frames derived from the D3D11 ones above (needs a fixed-size pool).
    pub fn derive_qsv(&mut self) -> BoxResult<()> {
        // SAFETY: derives from our live D3D11 device/frames refs; out-pointers are fields of self.
        unsafe {
            check(
                ff::av_hwdevice_ctx_create_derived(&mut self.qsv_device, QSV, self.device, 0),
                "derive QSV device",
            )?;
            check(
                ff::av_hwframe_ctx_create_derived(
                    &mut self.qsv_frames,
                    ff::AVPixelFormat::AV_PIX_FMT_QSV,
                    self.qsv_device,
                    self.frames,
                    0,
                ),
                "derive QSV frames",
            )?;
        }
        Ok(())
    }
}

impl Drop for HwRefs {
    fn drop(&mut self) {
        self.release();
    }
}
