//! FFmpeg D3D11VA device context around the caller's device, and the `get_format` callback that
//! builds a frames pool the presenter can sample (`D3D11_BIND_SHADER_RESOURCE`).

use std::ptr;

use ffmpeg_sys_next as ff;
use windows::Win32::Graphics::Direct3D11::{D3D11_BIND_SHADER_RESOURCE, ID3D11Device};
use windows::core::Interface;

use crate::CodecError;
use crate::decoder::EXTRA_SURFACES;
use crate::ffi::check;

const D3D11: ff::AVPixelFormat = ff::AVPixelFormat::AV_PIX_FMT_D3D11;

/// Shared with `get_format` through `AVCodecContext.opaque`; lives in a `Box` owned by the decoder.
#[derive(Debug, Default)]
pub(crate) struct FormatState {
    /// Always copy decoded frames into own textures (set before the first packet).
    pub(crate) force_copy: bool,
    /// The pool is not sampleable: frames go through `GpuCopy`.
    pub(crate) gpu_copy: bool,
    /// FFmpeg offered no D3D11 format or the pool could not be made.
    pub(crate) failed: bool,
}

/// New D3D11VA device context wrapping `device` (FFmpeg takes its own COM reference).
pub(crate) fn device_context(device: &ID3D11Device) -> Result<*mut ff::AVBufferRef, CodecError> {
    // SAFETY: FFmpeg owns the context; it releases `device` when the context is freed, so it gets
    // an AddRef'd pointer. On error the half-built context is unreferenced here.
    unsafe {
        let mut ctx = ff::av_hwdevice_ctx_alloc(ff::AVHWDeviceType::AV_HWDEVICE_TYPE_D3D11VA);
        if ctx.is_null() {
            return Err(CodecError::OutOfMemory);
        }
        let hw = (*ctx).data as *mut ff::AVHWDeviceContext;
        let d3d = (*hw).hwctx as *mut ff::AVD3D11VADeviceContext;
        (*d3d).device = device.clone().into_raw() as *mut ff::ID3D11Device;
        if let Err(e) = check(ff::av_hwdevice_ctx_init(ctx), "av_hwdevice_ctx_init") {
            ff::av_buffer_unref(&mut ctx);
            return Err(e);
        }
        Ok(ctx)
    }
}

/// Frames pool for the decoder; `sampleable` adds the shader-resource bind flag.
///
/// # Safety
/// `ctx` must be a live decoder context with `hw_device_ctx` set, inside `get_format`.
unsafe fn frames_context(ctx: *mut ff::AVCodecContext, sampleable: bool) -> *mut ff::AVBufferRef {
    let mut frames = ptr::null_mut();
    // SAFETY: per the function contract; `frames` is a valid out-pointer, freed on every error path.
    unsafe {
        if ff::avcodec_get_hw_frames_parameters(ctx, (*ctx).hw_device_ctx, D3D11, &mut frames) < 0 {
            return ptr::null_mut();
        }
        let fc = (*frames).data as *mut ff::AVHWFramesContext;
        (*fc).initial_pool_size += EXTRA_SURFACES as i32;
        if sampleable {
            let d3d = (*fc).hwctx as *mut ff::AVD3D11VAFramesContext;
            (*d3d).BindFlags |= D3D11_BIND_SHADER_RESOURCE.0 as u32;
        }
        if ff::av_hwframe_ctx_init(frames) < 0 {
            ff::av_buffer_unref(&mut frames);
        }
    }
    frames
}

/// `AVCodecContext.get_format`: D3D11 frames, sampleable when the driver allows it.
///
/// # Safety
/// Called by FFmpeg with a live context whose `opaque` points to a `FormatState`.
pub(crate) unsafe extern "C" fn get_format(
    ctx: *mut ff::AVCodecContext,
    formats: *const ff::AVPixelFormat,
) -> ff::AVPixelFormat {
    // SAFETY: FFmpeg passes a live context and a list ended by AV_PIX_FMT_NONE; `opaque` is the
    // decoder's boxed `FormatState`, alive as long as the context.
    unsafe {
        let state = &mut *((*ctx).opaque as *mut FormatState);
        let mut offered = false;
        let mut p = formats;
        while *p != ff::AVPixelFormat::AV_PIX_FMT_NONE {
            offered |= *p == D3D11;
            p = p.add(1);
        }
        if !offered {
            state.failed = true;
            return ff::AVPixelFormat::AV_PIX_FMT_NONE;
        }
        // Called again on stream changes: the old pool goes, a new one is made.
        ff::av_buffer_unref(&mut (*ctx).hw_frames_ctx);
        let mut frames = ptr::null_mut();
        if !state.force_copy {
            frames = frames_context(ctx, true);
        }
        state.gpu_copy = frames.is_null();
        if frames.is_null() {
            frames = frames_context(ctx, false);
        }
        if frames.is_null() {
            state.failed = true;
            return ff::AVPixelFormat::AV_PIX_FMT_NONE;
        }
        (*ctx).hw_frames_ctx = frames;
        D3D11
    }
}
