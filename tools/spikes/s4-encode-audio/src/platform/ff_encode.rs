//! FFmpeg hardware encoder fed with D3D11 textures from FFmpeg's own pool on our device. No CPU copy:
//! NVENC takes `AV_PIX_FMT_D3D11` frames; QSV takes QSV frames mapped (direct) from the same textures.

use std::ffi::{CString, c_void};
use std::ptr;

use ffmpeg_sys_next as ff;
use windows::Win32::Graphics::Direct3D11::{
    D3D11_BIND_RENDER_TARGET, ID3D11Device, ID3D11Texture2D,
};
use windows::core::Interface;

use crate::args::{Backend, EncodeArgs, Format};
use crate::platform::ff_util::{EAGAIN, Options, check};
use crate::util::BoxResult;

const POOL_SIZE: i32 = 8;

pub struct HwEncoder {
    ctx: *mut ff::AVCodecContext,
    device: *mut ff::AVBufferRef,
    frames: *mut ff::AVBufferRef,
    qsv_device: *mut ff::AVBufferRef,
    qsv_frames: *mut ff::AVBufferRef,
    frame: *mut ff::AVFrame,
    mapped: *mut ff::AVFrame,
    pkt: *mut ff::AVPacket,
    /// How frames reach the encoder, for the summary.
    pub path: &'static str,
}

fn options(a: &EncodeArgs) -> Vec<(&'static str, String)> {
    match a.backend {
        Backend::Nvenc => vec![
            ("preset", "p1".into()),
            ("tune", "ull".into()),
            ("rc", "cbr".into()),
            ("zerolatency", "1".into()),
            ("delay", "0".into()),
            ("intra-refresh", "1".into()),
            ("forced-idr", "0".into()),
        ],
        Backend::Qsv => vec![
            ("preset", "veryfast".into()),
            ("low_power", "1".into()),
            ("async_depth", "1".into()),
            ("look_ahead", "0".into()),
            ("int_ref_type", "vertical".into()),
        ],
    }
}

impl HwEncoder {
    pub fn open(dev: &ID3D11Device, a: &EncodeArgs) -> BoxResult<Self> {
        let mut enc = Self {
            ctx: ptr::null_mut(),
            device: ptr::null_mut(),
            frames: ptr::null_mut(),
            qsv_device: ptr::null_mut(),
            qsv_frames: ptr::null_mut(),
            // SAFETY: plain allocations, null-checked below.
            frame: unsafe { ff::av_frame_alloc() },
            mapped: unsafe { ff::av_frame_alloc() },
            pkt: unsafe { ff::av_packet_alloc() },
            path: "d3d11 frames",
        };
        if enc.frame.is_null() || enc.mapped.is_null() || enc.pkt.is_null() {
            return Err("out of memory".into());
        }
        enc.open_device(dev)?;
        enc.open_frames(a)?;
        if a.backend == Backend::Qsv {
            enc.derive_qsv()?;
        }
        enc.open_codec(a)?;
        Ok(enc)
    }

    fn open_device(&mut self, dev: &ID3D11Device) -> BoxResult<()> {
        // SAFETY: FFmpeg owns the device ref; it releases `device` on free, so we hand it an AddRef'd pointer.
        unsafe {
            self.device = ff::av_hwdevice_ctx_alloc(ff::AVHWDeviceType::AV_HWDEVICE_TYPE_D3D11VA);
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

    fn open_frames(&mut self, a: &EncodeArgs) -> BoxResult<()> {
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
            // NV12 array textures cannot carry the render-target bind flag (E_INVALIDARG on NVIDIA);
            // pool size 0 makes FFmpeg allocate one single texture per frame instead.
            (*fc).initial_pool_size = match a.format {
                Format::Bgra => POOL_SIZE,
                Format::Nv12 => 0,
            };
            let d3d = (*fc).hwctx as *mut ff::AVD3D11VAFramesContext;
            (*d3d).BindFlags = D3D11_BIND_RENDER_TARGET.0 as u32;
            check(ff::av_hwframe_ctx_init(self.frames), "av_hwframe_ctx_init")?;
        }
        Ok(())
    }

    fn derive_qsv(&mut self) -> BoxResult<()> {
        // SAFETY: derives from our live D3D11 device/frames refs; out-pointers are fields of self.
        unsafe {
            let qsv = ff::AVHWDeviceType::AV_HWDEVICE_TYPE_QSV;
            check(
                ff::av_hwdevice_ctx_create_derived(&mut self.qsv_device, qsv, self.device, 0),
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
        self.path = "qsv frames mapped (direct) from d3d11 frames";
        Ok(())
    }

    fn open_codec(&mut self, a: &EncodeArgs) -> BoxResult<()> {
        let name = CString::new(a.encoder_name())?;
        let mut opts = Options::new(&options(a))?;
        // SAFETY: codec context allocated here and owned by self; refs taken with av_buffer_ref.
        unsafe {
            let codec = ff::avcodec_find_encoder_by_name(name.as_ptr());
            if codec.is_null() {
                return Err(
                    format!("encoder {} not in this FFmpeg build", a.encoder_name()).into(),
                );
            }
            self.ctx = ff::avcodec_alloc_context3(codec);
            if self.ctx.is_null() {
                return Err("avcodec_alloc_context3 failed".into());
            }
            let c = &mut *self.ctx;
            c.width = a.width as i32;
            c.height = a.height as i32;
            c.time_base = ff::AVRational {
                num: 1,
                den: a.fps as i32,
            };
            c.framerate = ff::AVRational {
                num: a.fps as i32,
                den: 1,
            };
            c.bit_rate = a.bitrate as i64;
            c.rc_max_rate = a.bitrate as i64;
            c.rc_buffer_size = (a.bitrate / u64::from(a.fps)) as i32;
            c.gop_size = a.fps as i32;
            c.max_b_frames = 0;
            let src = if self.qsv_frames.is_null() {
                self.frames
            } else {
                self.qsv_frames
            };
            c.pix_fmt = if self.qsv_frames.is_null() {
                ff::AVPixelFormat::AV_PIX_FMT_D3D11
            } else {
                ff::AVPixelFormat::AV_PIX_FMT_QSV
            };
            c.hw_frames_ctx = ff::av_buffer_ref(src);
            // avcodec_open2 replaces the dictionary with the options it did not consume.
            check(
                ff::avcodec_open2(self.ctx, codec, &mut opts.0),
                "avcodec_open2",
            )?;
            let unknown = opts.leftover_keys();
            if !unknown.is_empty() {
                return Err(format!("encoder ignored options: {}", unknown.join(", ")).into());
            }
        }
        Ok(())
    }

    /// Next free pool texture and its array slice (the encoder may still hold others).
    pub fn next_frame(&mut self) -> BoxResult<(ID3D11Texture2D, u32)> {
        // SAFETY: frame is ours; data[0] is the pool's ID3D11Texture2D, data[1] the array index.
        unsafe {
            ff::av_frame_unref(self.frame);
            check(
                ff::av_hwframe_get_buffer(self.frames, self.frame, 0),
                "av_hwframe_get_buffer",
            )?;
            let raw = (*self.frame).data[0] as *mut c_void;
            let tex = ID3D11Texture2D::from_raw_borrowed(&raw).ok_or("frame has no texture")?;
            Ok((tex.clone(), (*self.frame).data[1] as usize as u32))
        }
    }

    // HOT PATH
    pub fn send(&mut self, pts: i64) -> BoxResult<()> {
        // SAFETY: frames are ours; the encoder takes its own references.
        unsafe {
            (*self.frame).pts = pts;
            if self.qsv_frames.is_null() {
                return check(
                    ff::avcodec_send_frame(self.ctx, self.frame),
                    "avcodec_send_frame",
                )
                .map(|_| ());
            }
            ff::av_frame_unref(self.mapped);
            (*self.mapped).format = ff::AVPixelFormat::AV_PIX_FMT_QSV as i32;
            (*self.mapped).hw_frames_ctx = ff::av_buffer_ref(self.qsv_frames);
            let flags = ff::AV_HWFRAME_MAP_READ as i32 | ff::AV_HWFRAME_MAP_DIRECT as i32;
            check(
                ff::av_hwframe_map(self.mapped, self.frame, flags),
                "av_hwframe_map",
            )?;
            (*self.mapped).pts = pts;
            check(
                ff::avcodec_send_frame(self.ctx, self.mapped),
                "avcodec_send_frame",
            )?;
        }
        Ok(())
    }

    /// Hands one ready packet (pts, bytes) to `sink`. `Ok(false)` = none ready yet (or drained).
    // HOT PATH
    pub fn poll(&mut self, sink: &mut dyn FnMut(i64, &[u8])) -> BoxResult<bool> {
        // SAFETY: packet is ours; data/size valid until unref.
        unsafe {
            let ret = ff::avcodec_receive_packet(self.ctx, self.pkt);
            if ret == EAGAIN || ret == ff::AVERROR_EOF {
                return Ok(false);
            }
            check(ret, "avcodec_receive_packet")?;
            let p = &*self.pkt;
            let data = if p.data.is_null() {
                &[][..]
            } else {
                std::slice::from_raw_parts(p.data, p.size as usize)
            };
            sink(p.pts, data);
            ff::av_packet_unref(self.pkt);
        }
        Ok(true)
    }

    /// Ends the stream and hands every remaining packet to `sink`.
    pub fn flush(&mut self, sink: &mut dyn FnMut(i64, &[u8])) -> BoxResult<()> {
        // SAFETY: a null frame starts draining.
        check(
            unsafe { ff::avcodec_send_frame(self.ctx, ptr::null()) },
            "flush",
        )?;
        while self.poll(sink)? {}
        Ok(())
    }
}

impl Drop for HwEncoder {
    fn drop(&mut self) {
        // SAFETY: every pointer is owned here (or null); FFmpeg's free functions accept null.
        unsafe {
            ff::avcodec_free_context(&mut self.ctx);
            ff::av_frame_free(&mut self.mapped);
            ff::av_frame_free(&mut self.frame);
            ff::av_packet_free(&mut self.pkt);
            ff::av_buffer_unref(&mut self.qsv_frames);
            ff::av_buffer_unref(&mut self.qsv_device);
            ff::av_buffer_unref(&mut self.frames);
            ff::av_buffer_unref(&mut self.device);
        }
    }
}
