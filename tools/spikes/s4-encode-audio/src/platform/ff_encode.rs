//! FFmpeg hardware encoder fed with D3D11 textures from an FFmpeg pool. No CPU copy on any path:
//! - NVENC: `AV_PIX_FMT_D3D11` frames from a pool on our device.
//! - QSV, derived: QSV device + frames derived from our device and pool; frames mapped (direct).
//! - QSV, owned (fallback when deriving fails): FFmpeg creates the QSV device with a D3D11 child on the
//!   Intel adapter and a QSV pool; we draw into the D3D11 textures mapped (direct) from its surfaces.

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
/// `MFX_MEMTYPE_VIDEO_MEMORY_PROCESSOR_TARGET` (mfxstructures.h): FFmpeg then creates the D3D11 child
/// textures with the render-target bind flag.
const MFX_MEMTYPE_VIDEO_MEMORY_PROCESSOR_TARGET: i32 = 0x0020;
const D3D11VA: ff::AVHWDeviceType = ff::AVHWDeviceType::AV_HWDEVICE_TYPE_D3D11VA;
const QSV: ff::AVHWDeviceType = ff::AVHWDeviceType::AV_HWDEVICE_TYPE_QSV;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Feed {
    D3d11,
    QsvDerived,
    QsvOwned,
}

pub struct HwEncoder {
    ctx: *mut ff::AVCodecContext,
    device: *mut ff::AVBufferRef,
    frames: *mut ff::AVBufferRef,
    qsv_device: *mut ff::AVBufferRef,
    qsv_frames: *mut ff::AVBufferRef,
    /// D3D11 frame we draw into (pool frame, or mapped from `qsv_frame` on the owned path).
    frame: *mut ff::AVFrame,
    /// QSV frame sent to the encoder (mapped from `frame`, or the pool frame on the owned path).
    qsv_frame: *mut ff::AVFrame,
    pkt: *mut ff::AVPacket,
    feed: Feed,
    /// How frames reach the encoder, for the summary.
    pub path: String,
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
    /// `adapter_index`: DXGI enumeration index of the adapter (QSV owned path only).
    pub fn open(dev: &ID3D11Device, a: &EncodeArgs, adapter_index: u32) -> BoxResult<Self> {
        let mut enc = Self {
            ctx: ptr::null_mut(),
            device: ptr::null_mut(),
            frames: ptr::null_mut(),
            qsv_device: ptr::null_mut(),
            qsv_frames: ptr::null_mut(),
            // SAFETY: plain allocations, null-checked below.
            frame: unsafe { ff::av_frame_alloc() },
            qsv_frame: unsafe { ff::av_frame_alloc() },
            pkt: unsafe { ff::av_packet_alloc() },
            feed: Feed::D3d11,
            path: "d3d11 frames".into(),
        };
        if enc.frame.is_null() || enc.qsv_frame.is_null() || enc.pkt.is_null() {
            return Err("out of memory".into());
        }
        if a.backend == Backend::Nvenc {
            enc.open_device(dev)?;
            enc.open_frames(a)?;
        } else {
            let derived = enc
                .open_device(dev)
                .and_then(|_| enc.open_frames(a))
                .and_then(|_| enc.derive_qsv());
            match derived {
                Ok(()) => {
                    enc.feed = Feed::QsvDerived;
                    enc.path = "qsv derived from our d3d11 device, frames mapped (direct)".into();
                }
                Err(e) => {
                    enc.release_hw();
                    enc.open_qsv_owned(a, adapter_index)?;
                    enc.feed = Feed::QsvOwned;
                    enc.path = format!(
                        "qsv-owned device (d3d11 child, adapter {adapter_index}), d3d11 textures mapped (direct) from qsv surfaces; deriving from our device failed: {e}"
                    );
                }
            }
        }
        enc.open_codec(a)?;
        Ok(enc)
    }

    /// The D3D11 device the textures live on (draw with this one).
    pub fn device(&self) -> BoxResult<ID3D11Device> {
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

    fn release_hw(&mut self) {
        // SAFETY: owned refs (or null).
        unsafe {
            ff::av_buffer_unref(&mut self.qsv_frames);
            ff::av_buffer_unref(&mut self.qsv_device);
            ff::av_buffer_unref(&mut self.frames);
            ff::av_buffer_unref(&mut self.device);
        }
    }

    fn open_device(&mut self, dev: &ID3D11Device) -> BoxResult<()> {
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

    /// QSV fallback: FFmpeg creates the QSV device itself (D3D11 child on the Intel adapter) and the pool.
    fn open_qsv_owned(&mut self, a: &EncodeArgs, adapter_index: u32) -> BoxResult<()> {
        // The device string picks the MFX implementation; the adapter goes to the D3D11 child.
        let opts = Options::new(&[
            ("child_device_type", "d3d11va".into()),
            ("child_device", adapter_index.to_string()),
        ])?;
        // SAFETY: refs created here are owned by self; fields set before av_hwframe_ctx_init.
        unsafe {
            check(
                ff::av_hwdevice_ctx_create(&mut self.qsv_device, QSV, c"hw".as_ptr(), opts.0, 0),
                "create QSV device (child d3d11va)",
            )?;
            check(
                ff::av_hwdevice_ctx_create_derived(&mut self.device, D3D11VA, self.qsv_device, 0),
                "get the QSV device's D3D11 child",
            )?;
            self.qsv_frames = ff::av_hwframe_ctx_alloc(self.qsv_device);
            if self.qsv_frames.is_null() {
                return Err("av_hwframe_ctx_alloc (QSV) failed".into());
            }
            let fc = (*self.qsv_frames).data as *mut ff::AVHWFramesContext;
            (*fc).format = ff::AVPixelFormat::AV_PIX_FMT_QSV;
            (*fc).sw_format = ff::AVPixelFormat::AV_PIX_FMT_NV12;
            (*fc).width = a.width as i32;
            (*fc).height = a.height as i32;
            (*fc).initial_pool_size = POOL_SIZE;
            let q = (*fc).hwctx as *mut ff::AVQSVFramesContext;
            (*q).frame_type = MFX_MEMTYPE_VIDEO_MEMORY_PROCESSOR_TARGET;
            check(
                ff::av_hwframe_ctx_init(self.qsv_frames),
                "av_hwframe_ctx_init (QSV)",
            )?;
        }
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
            let (src, fmt) = if self.feed == Feed::D3d11 {
                (self.frames, ff::AVPixelFormat::AV_PIX_FMT_D3D11)
            } else {
                (self.qsv_frames, ff::AVPixelFormat::AV_PIX_FMT_QSV)
            };
            c.pix_fmt = fmt;
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
        // SAFETY: frames are ours; a D3D11 frame has the ID3D11Texture2D in data[0], the slice in data[1].
        unsafe {
            ff::av_frame_unref(self.frame);
            if self.feed == Feed::QsvOwned {
                ff::av_frame_unref(self.qsv_frame);
                check(
                    ff::av_hwframe_get_buffer(self.qsv_frames, self.qsv_frame, 0),
                    "av_hwframe_get_buffer (QSV)",
                )?;
                (*self.frame).format = ff::AVPixelFormat::AV_PIX_FMT_D3D11 as i32;
                let flags = ff::AV_HWFRAME_MAP_WRITE as i32 | ff::AV_HWFRAME_MAP_DIRECT as i32;
                check(
                    ff::av_hwframe_map(self.frame, self.qsv_frame, flags),
                    "map QSV surface to D3D11",
                )?;
            } else {
                check(
                    ff::av_hwframe_get_buffer(self.frames, self.frame, 0),
                    "av_hwframe_get_buffer",
                )?;
            }
            let raw = (*self.frame).data[0] as *mut c_void;
            let tex = ID3D11Texture2D::from_raw_borrowed(&raw).ok_or("frame has no texture")?;
            Ok((tex.clone(), (*self.frame).data[1] as usize as u32))
        }
    }

    // HOT PATH
    pub fn send(&mut self, pts: i64) -> BoxResult<()> {
        // SAFETY: frames are ours; the encoder takes its own references.
        unsafe {
            match self.feed {
                Feed::D3d11 => {
                    (*self.frame).pts = pts;
                    check(
                        ff::avcodec_send_frame(self.ctx, self.frame),
                        "avcodec_send_frame",
                    )?;
                }
                Feed::QsvDerived => {
                    ff::av_frame_unref(self.qsv_frame);
                    (*self.qsv_frame).format = ff::AVPixelFormat::AV_PIX_FMT_QSV as i32;
                    (*self.qsv_frame).hw_frames_ctx = ff::av_buffer_ref(self.qsv_frames);
                    let flags = ff::AV_HWFRAME_MAP_READ as i32 | ff::AV_HWFRAME_MAP_DIRECT as i32;
                    check(
                        ff::av_hwframe_map(self.qsv_frame, self.frame, flags),
                        "av_hwframe_map",
                    )?;
                    (*self.qsv_frame).pts = pts;
                    check(
                        ff::avcodec_send_frame(self.ctx, self.qsv_frame),
                        "avcodec_send_frame",
                    )?;
                }
                Feed::QsvOwned => {
                    (*self.qsv_frame).pts = pts;
                    check(
                        ff::avcodec_send_frame(self.ctx, self.qsv_frame),
                        "avcodec_send_frame",
                    )?;
                }
            }
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
            ff::av_frame_free(&mut self.frame);
            ff::av_frame_free(&mut self.qsv_frame);
            ff::av_packet_free(&mut self.pkt);
        }
        self.release_hw();
    }
}
