//! The FFmpeg decoder context with D3D11VA hardware frames on the caller's device.

use std::ffi::c_void;
use std::ptr;
use std::sync::Arc;

use ffmpeg_sys_next as ff;
use windows::Win32::Graphics::Direct3D11::{ID3D11Device, ID3D11Texture2D};
use windows::core::Interface;

use super::copy::GpuCopy;
use super::frame_pool::{FramePool, PooledFrame};
use super::hwctx::{FormatState, device_context, get_format};
use crate::ffi::{EAGAIN, check, codec_id};
use crate::{CodecError, VideoCodec};

const PADDING: usize = ff::AV_INPUT_BUFFER_PADDING_SIZE as usize;

/// One decoded picture: an `AVFrame` reference plus the texture to sample.
#[derive(Debug)]
pub(crate) struct RawFrame {
    pub(crate) texture: ID3D11Texture2D,
    pub(crate) index: u32,
    pub(crate) width: u32,
    pub(crate) height: u32,
    // Keeps the pool surface (and the device) alive; returned to the pool on drop.
    pub(crate) _frame: PooledFrame,
}

// SAFETY: D3D11 textures are free-threaded; the pooled frame is `Send`.
unsafe impl Send for RawFrame {}

#[derive(Debug)]
pub(crate) struct D3d11vaDecoder {
    codec: VideoCodec,
    ctx: *mut ff::AVCodecContext,
    pkt: *mut ff::AVPacket,
    /// Access unit plus FFmpeg's zeroed read-ahead padding.
    padded: Vec<u8>,
    state: Box<FormatState>,
    pool: Arc<FramePool>,
    copy: GpuCopy,
}

impl D3d11vaDecoder {
    pub(crate) fn new(
        device: &ID3D11Device,
        codec: VideoCodec,
        force_copy: bool,
    ) -> Result<Self, CodecError> {
        if !super::probe::has_decoder(device, codec) {
            return Err(CodecError::NoHardwareDecoder(codec));
        }
        let mut this = Self {
            codec,
            ctx: ptr::null_mut(),
            // SAFETY: plain allocation, null-checked below.
            pkt: unsafe { ff::av_packet_alloc() },
            padded: Vec::with_capacity(512 * 1024),
            state: Box::new(FormatState {
                force_copy,
                ..FormatState::default()
            }),
            pool: FramePool::new()?,
            copy: GpuCopy::new(device)?,
        };
        if this.pkt.is_null() {
            return Err(CodecError::OutOfMemory);
        }
        let mut hw_device = device_context(device)?;
        let opened = this.open(hw_device);
        // SAFETY: our reference; the codec context holds its own one when opened.
        unsafe { ff::av_buffer_unref(&mut hw_device) };
        opened?;
        Ok(this)
    }

    fn open(&mut self, hw_device: *mut ff::AVBufferRef) -> Result<(), CodecError> {
        // SAFETY: the decoder pointer is static FFmpeg data; the context is null-checked and owned
        // by `self` (freed in Drop); `state` is boxed, so `opaque` stays valid while `self` lives.
        unsafe {
            let decoder = ff::avcodec_find_decoder(codec_id(self.codec));
            self.ctx = ff::avcodec_alloc_context3(decoder);
            if decoder.is_null() || self.ctx.is_null() {
                return Err(CodecError::OutOfMemory);
            }
            let ctx = &mut *self.ctx;
            ctx.hw_device_ctx = ff::av_buffer_ref(hw_device);
            ctx.opaque = (&mut *self.state as *mut FormatState).cast::<c_void>();
            ctx.get_format = Some(get_format);
            ctx.flags |= ff::AV_CODEC_FLAG_LOW_DELAY as i32;
            ctx.flags2 |= ff::AV_CODEC_FLAG2_FAST;
            ctx.thread_count = 1;
            check(
                ff::avcodec_open2(self.ctx, decoder, ptr::null_mut()),
                "avcodec_open2",
            )?;
        }
        Ok(())
    }

    pub(crate) fn uses_gpu_copy(&self) -> bool {
        self.state.gpu_copy
    }

    // HOT PATH
    pub(crate) fn decode(&mut self, au: &[u8]) -> Result<Option<RawFrame>, CodecError> {
        // FFmpeg may read up to PADDING bytes past the data; the copy keeps that inside our buffer.
        self.padded.clear();
        self.padded.extend_from_slice(au);
        self.padded.resize(au.len() + PADDING, 0);
        let size = i32::try_from(au.len()).map_err(|_| CodecError::Corrupt)?;
        // SAFETY: context and packet are live; the packet borrows `padded` only for this call
        // (not reference-counted, so FFmpeg copies what it keeps).
        let sent = unsafe {
            (*self.pkt).data = self.padded.as_mut_ptr();
            (*self.pkt).size = size;
            let sent = ff::avcodec_send_packet(self.ctx, self.pkt);
            (*self.pkt).data = ptr::null_mut();
            (*self.pkt).size = 0;
            sent
        };
        if self.state.failed {
            return Err(CodecError::NoHardwareDecoder(self.codec));
        }
        if sent < 0 && sent != EAGAIN {
            return Err(CodecError::Corrupt);
        }
        self.receive()
    }

    fn receive(&mut self) -> Result<Option<RawFrame>, CodecError> {
        let Some(frame) = self.pool.take() else {
            return Ok(None);
        };
        // SAFETY: live context; `frame` is an empty pooled frame.
        let got = unsafe { ff::avcodec_receive_frame(self.ctx, frame.as_ptr()) };
        if got == EAGAIN || got == ff::AVERROR_EOF {
            return Ok(None);
        }
        if got < 0 {
            return Err(CodecError::Corrupt);
        }
        // SAFETY: a D3D11 hw frame: data[0] is the ID3D11Texture2D of the pool, data[1] the slice.
        let (texture, index, width, height) = unsafe {
            let f = &*frame.as_ptr();
            let raw = f.data[0].cast::<c_void>();
            let texture = ID3D11Texture2D::from_raw_borrowed(&raw)
                .ok_or(CodecError::Corrupt)?
                .clone();
            let index = u32::try_from(f.data[1] as usize).map_err(|_| CodecError::Corrupt)?;
            let width = u32::try_from(f.width).map_err(|_| CodecError::Corrupt)?;
            let height = u32::try_from(f.height).map_err(|_| CodecError::Corrupt)?;
            (texture, index, width, height)
        };
        let (texture, index) = if self.state.gpu_copy {
            let even = |v: u32| (v + 1) & !1;
            let copy = self.copy.copy(&texture, index, even(width), even(height))?;
            (copy, 0)
        } else {
            (texture, index)
        };
        Ok(Some(RawFrame {
            texture,
            index,
            width,
            height,
            _frame: frame,
        }))
    }
}

// SAFETY: the FFmpeg contexts are owned by this value and only used through `&mut self`;
// D3D11 objects are free-threaded.
unsafe impl Send for D3d11vaDecoder {}

impl Drop for D3d11vaDecoder {
    fn drop(&mut self) {
        // SAFETY: both are owned here (or null); the free functions accept null.
        unsafe {
            ff::avcodec_free_context(&mut self.ctx);
            ff::av_packet_free(&mut self.pkt);
        }
    }
}
