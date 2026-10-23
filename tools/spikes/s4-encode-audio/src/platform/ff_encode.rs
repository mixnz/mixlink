//! FFmpeg hardware encoder fed with D3D11 textures from an FFmpeg pool. No CPU copy on any path:
//! - NVENC: `AV_PIX_FMT_D3D11` frames from a pool on our device.
//! - QSV, derived: QSV device + frames derived from our device and pool; frames mapped (direct).
//! - QSV, owned (fallback when deriving fails): FFmpeg creates the QSV device with a D3D11 child on the
//!   Intel adapter and a QSV pool; we draw into the D3D11 textures mapped (direct) from its surfaces.

use std::ffi::{CString, c_void};
use std::ptr;

use ffmpeg_sys_next as ff;
use windows::Win32::Graphics::Direct3D11::{ID3D11Device, ID3D11Texture2D};
use windows::core::Interface;

use crate::args::{Backend, EncodeArgs};
use crate::platform::ff_hwctx::HwRefs;
use crate::platform::ff_util::{EAGAIN, Options, check};
use crate::util::BoxResult;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Feed {
    D3d11,
    QsvDerived,
    QsvOwned,
}

pub struct HwEncoder {
    ctx: *mut ff::AVCodecContext,
    hw: HwRefs,
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
            hw: HwRefs::default(),
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
            enc.hw.open_d3d11(dev)?;
            enc.hw.open_d3d11_frames(a)?;
        } else {
            let derived = enc
                .hw
                .open_d3d11(dev)
                .and_then(|_| enc.hw.open_d3d11_frames(a))
                .and_then(|_| enc.hw.derive_qsv());
            match derived {
                Ok(()) => {
                    enc.feed = Feed::QsvDerived;
                    enc.path = "qsv derived from our d3d11 device, frames mapped (direct)".into();
                }
                Err(e) => {
                    enc.hw.release();
                    enc.hw.open_qsv_owned(a, adapter_index)?;
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
        self.hw.d3d11_device()
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
                (self.hw.frames, ff::AVPixelFormat::AV_PIX_FMT_D3D11)
            } else {
                (self.hw.qsv_frames, ff::AVPixelFormat::AV_PIX_FMT_QSV)
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
                    ff::av_hwframe_get_buffer(self.hw.qsv_frames, self.qsv_frame, 0),
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
                    ff::av_hwframe_get_buffer(self.hw.frames, self.frame, 0),
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
                    (*self.qsv_frame).hw_frames_ctx = ff::av_buffer_ref(self.hw.qsv_frames);
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
    }
}
