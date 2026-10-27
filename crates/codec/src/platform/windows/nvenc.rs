//! FFmpeg `h264_nvenc` / `hevc_nvenc` fed with the caller's D3D11 textures (spec 0019): no FFmpeg
//! frame pool, no copy. Each `AVFrame` points at the caller's texture; NVENC registers it once.

use std::ffi::CStr;
use std::ptr;

use ffmpeg_sys_next as ff;
use windows::Win32::Graphics::Direct3D11::{ID3D11Device, ID3D11Texture2D};
use windows::core::Interface;

use super::hwctx::device_context;
use crate::encoder::EncoderConfig;
use crate::ffi::{EAGAIN, check};
use crate::{CodecError, VideoCodec};

/// Largest access unit kept (a 4 MiB keyframe, the video reassembler's limit).
pub(crate) const AU_CAPACITY: usize = 4 * 1024 * 1024;

const OPTIONS: [(&CStr, &CStr); 6] = [
    (c"preset", c"p1"),
    (c"tune", c"ull"),
    (c"rc", c"cbr"),
    (c"zerolatency", c"1"),
    (c"delay", c"0"),
    (c"forced-idr", c"1"),
];

fn encoder_name(codec: VideoCodec) -> &'static CStr {
    match codec {
        VideoCodec::H264 => c"h264_nvenc",
        VideoCodec::Hevc => c"hevc_nvenc",
    }
}

fn int(value: u64, what: &'static str) -> Result<i32, CodecError> {
    i32::try_from(value).map_err(|_| CodecError::Ffmpeg {
        call: what,
        code: ff::AVERROR_EXTERNAL,
        text: format!("{value} is out of range"),
    })
}

#[derive(Debug)]
pub(crate) struct NvencEncoder {
    ctx: *mut ff::AVCodecContext,
    device_ref: *mut ff::AVBufferRef,
    frames_ref: *mut ff::AVBufferRef,
    frame: *mut ff::AVFrame,
    pkt: *mut ff::AVPacket,
    /// Stands in for `frame.buf[0]`: FFmpeg only encodes frames that carry a buffer.
    dummy: *mut ff::AVBufferRef,
    pts: i64,
}

// SAFETY: every FFmpeg object is owned here and used by one thread at a time (`&mut self`).
unsafe impl Send for NvencEncoder {}

impl NvencEncoder {
    pub(crate) fn open(device: &ID3D11Device, cfg: &EncoderConfig) -> Result<Self, CodecError> {
        // SAFETY: plain allocations, null-checked below; Drop frees whatever was allocated.
        let mut enc = unsafe {
            Self {
                ctx: ptr::null_mut(),
                device_ref: ptr::null_mut(),
                frames_ref: ptr::null_mut(),
                frame: ff::av_frame_alloc(),
                pkt: ff::av_packet_alloc(),
                dummy: ff::av_buffer_allocz(1),
                pts: 0,
            }
        };
        if enc.frame.is_null() || enc.pkt.is_null() || enc.dummy.is_null() {
            return Err(CodecError::OutOfMemory);
        }
        enc.device_ref = device_context(device)?;
        enc.open_frames(cfg)?;
        enc.open_codec(cfg)?;
        enc.prepare_frame(cfg)?;
        Ok(enc)
    }

    /// BGRA frames context of the encode size; never allocates textures (pool size 0).
    fn open_frames(&mut self, cfg: &EncoderConfig) -> Result<(), CodecError> {
        // SAFETY: the device context is live; the frames context is owned by `self`.
        unsafe {
            self.frames_ref = ff::av_hwframe_ctx_alloc(self.device_ref);
            if self.frames_ref.is_null() {
                return Err(CodecError::OutOfMemory);
            }
            let fc = (*self.frames_ref).data as *mut ff::AVHWFramesContext;
            (*fc).format = ff::AVPixelFormat::AV_PIX_FMT_D3D11;
            (*fc).sw_format = ff::AVPixelFormat::AV_PIX_FMT_BGRA;
            (*fc).width = int(cfg.width.into(), "width")?;
            (*fc).height = int(cfg.height.into(), "height")?;
            (*fc).initial_pool_size = 0;
            check(
                ff::av_hwframe_ctx_init(self.frames_ref),
                "av_hwframe_ctx_init",
            )?;
        }
        Ok(())
    }

    fn open_codec(&mut self, cfg: &EncoderConfig) -> Result<(), CodecError> {
        let bitrate = u64::from(cfg.bitrate_kbps) * 1000;
        let mut opts: *mut ff::AVDictionary = ptr::null_mut();
        // SAFETY: the context is owned by `self`; the dictionary is freed on every path below.
        unsafe {
            let codec = ff::avcodec_find_encoder_by_name(encoder_name(cfg.codec).as_ptr());
            if codec.is_null() {
                return Err(CodecError::NoHardwareEncoder(cfg.codec));
            }
            self.ctx = ff::avcodec_alloc_context3(codec);
            if self.ctx.is_null() {
                return Err(CodecError::OutOfMemory);
            }
            let c = &mut *self.ctx;
            c.width = int(cfg.width.into(), "width")?;
            c.height = int(cfg.height.into(), "height")?;
            c.time_base = ff::AVRational {
                num: 1,
                den: int(cfg.fps.into(), "fps")?,
            };
            c.framerate = ff::AVRational {
                num: int(cfg.fps.into(), "fps")?,
                den: 1,
            };
            c.bit_rate = int(bitrate, "bitrate")?.into();
            c.rc_max_rate = c.bit_rate;
            c.rc_buffer_size = int(bitrate / u64::from(cfg.fps.max(1)), "vbv")?;
            c.gop_size = i32::MAX; // IDR only on the first frame and on request
            c.max_b_frames = 0;
            c.pix_fmt = ff::AVPixelFormat::AV_PIX_FMT_D3D11;
            // NVENC converts BGRA with the BT.601 matrix whatever is asked: tag it so (spec 0019).
            c.colorspace = ff::AVColorSpace::AVCOL_SPC_SMPTE170M;
            c.color_primaries = ff::AVColorPrimaries::AVCOL_PRI_BT709;
            c.color_trc = ff::AVColorTransferCharacteristic::AVCOL_TRC_BT709;
            c.color_range = ff::AVColorRange::AVCOL_RANGE_MPEG;
            c.hw_frames_ctx = ff::av_buffer_ref(self.frames_ref);
            if c.hw_frames_ctx.is_null() {
                return Err(CodecError::OutOfMemory);
            }
            for (key, value) in OPTIONS {
                ff::av_dict_set(&mut opts, key.as_ptr(), value.as_ptr(), 0);
            }
            // avcodec_open2 leaves the options it did not consume in the dictionary.
            let opened = check(
                ff::avcodec_open2(self.ctx, codec, &mut opts),
                "avcodec_open2",
            );
            let unknown = ff::av_dict_count(opts);
            ff::av_dict_free(&mut opts);
            opened.map_err(|e| match e {
                CodecError::Ffmpeg { .. } => CodecError::NoHardwareEncoder(cfg.codec),
                other => other,
            })?;
            if unknown > 0 {
                return Err(CodecError::Ffmpeg {
                    call: "avcodec_open2",
                    code: ff::AVERROR_OPTION_NOT_FOUND,
                    text: format!("{unknown} encoder option(s) not recognised"),
                });
            }
        }
        Ok(())
    }

    /// Fields that stay the same for every frame.
    fn prepare_frame(&mut self, cfg: &EncoderConfig) -> Result<(), CodecError> {
        // SAFETY: the frame is owned by `self`; buf[0] and hw_frames_ctx hold own references.
        unsafe {
            let f = &mut *self.frame;
            f.format = ff::AVPixelFormat::AV_PIX_FMT_D3D11 as i32;
            f.width = int(cfg.width.into(), "width")?;
            f.height = int(cfg.height.into(), "height")?;
            f.buf[0] = ff::av_buffer_ref(self.dummy);
            f.hw_frames_ctx = ff::av_buffer_ref(self.frames_ref);
            if f.buf[0].is_null() || f.hw_frames_ctx.is_null() {
                return Err(CodecError::OutOfMemory);
            }
        }
        Ok(())
    }

    // HOT PATH
    /// Encodes `texture` (BGRA, the configured size, on the encoder's device) and appends the access
    /// unit to `out` (cleared first; capacity `AU_CAPACITY`, never grown). When this returns, NVENC
    /// has read the texture.
    pub(crate) fn encode(
        &mut self,
        texture: &ID3D11Texture2D,
        force_idr: bool,
        out: &mut Vec<u8>,
    ) -> Result<(), CodecError> {
        out.clear();
        self.pts += 1;
        // SAFETY: the frame borrows `texture` only until avcodec_send_frame returns (FFmpeg takes its
        // own references to the frame's buffers); the packet is unreferenced before returning.
        unsafe {
            let f = &mut *self.frame;
            f.data[0] = texture.as_raw().cast();
            f.data[1] = ptr::null_mut(); // array slice 0
            f.pts = self.pts;
            f.pict_type = if force_idr {
                ff::AVPictureType::AV_PICTURE_TYPE_I
            } else {
                ff::AVPictureType::AV_PICTURE_TYPE_NONE
            };
            let sent = check(
                ff::avcodec_send_frame(self.ctx, self.frame),
                "avcodec_send_frame",
            );
            f.data[0] = ptr::null_mut();
            sent?;
            let ret = ff::avcodec_receive_packet(self.ctx, self.pkt);
            if ret == EAGAIN {
                // Low delay: one frame in must give one packet out.
                return Err(CodecError::Ffmpeg {
                    call: "avcodec_receive_packet",
                    code: ret,
                    text: "encoder held the frame back".into(),
                });
            }
            check(ret, "avcodec_receive_packet")?;
            let p = &*self.pkt;
            let len = usize::try_from(p.size).unwrap_or(0);
            let fits = len <= out.capacity() && !p.data.is_null();
            if fits {
                out.extend_from_slice(std::slice::from_raw_parts(p.data, len));
            }
            ff::av_packet_unref(self.pkt);
            if !fits {
                return Err(CodecError::AuTooLarge { len });
            }
        }
        Ok(())
    }
}

impl Drop for NvencEncoder {
    fn drop(&mut self) {
        // SAFETY: every pointer is owned here (or null); FFmpeg's free functions accept null.
        unsafe {
            ff::avcodec_free_context(&mut self.ctx);
            ff::av_frame_free(&mut self.frame);
            ff::av_packet_free(&mut self.pkt);
            ff::av_buffer_unref(&mut self.dummy);
            ff::av_buffer_unref(&mut self.frames_ref);
            ff::av_buffer_unref(&mut self.device_ref);
        }
    }
}
