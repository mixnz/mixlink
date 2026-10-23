//! FFmpeg hardware encoder fed with D3D11 textures from an FFmpeg pool. No CPU copy on any path:
//! - NVENC: `AV_PIX_FMT_D3D11` frames from a pool on our device.
//! - QSV: QSV device + frames derived from a D3D11 device and fixed pool, frames mapped (direct).
//!   Tried in order until one opens: our device / an FFmpeg-created device, pool with the render-target
//!   bind flag / the decoder bind flag (then we draw into an own texture and the GPU copies it in).

use std::ffi::{CString, c_void};
use std::ptr;

use ffmpeg_sys_next as ff;
use windows::Win32::Graphics::Direct3D11::{
    D3D11_BIND_DECODER, D3D11_BIND_RENDER_TARGET, ID3D11Device, ID3D11Texture2D,
};
use windows::core::Interface;

use crate::args::{Backend, EncodeArgs, Format};
use crate::platform::ff_hwctx::{HwRefs, POOL_SIZE};
use crate::platform::ff_util::{EAGAIN, Options, check};
use crate::util::BoxResult;

const BIND_RT: u32 = D3D11_BIND_RENDER_TARGET.0 as u32;
const BIND_DECODER: u32 = D3D11_BIND_DECODER.0 as u32;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Feed {
    D3d11,
    QsvDerived,
}

pub struct HwEncoder {
    ctx: *mut ff::AVCodecContext,
    hw: HwRefs,
    /// D3D11 pool frame we draw into.
    frame: *mut ff::AVFrame,
    /// QSV frame mapped from `frame`, sent to the encoder (QSV only).
    qsv_frame: *mut ff::AVFrame,
    pkt: *mut ff::AVPacket,
    feed: Feed,
    /// Pool textures lack the render-target flag: draw elsewhere and copy in (GPU).
    copy_draw: bool,
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
    /// `adapter_index`: DXGI enumeration index of the adapter (FFmpeg-created QSV device attempt).
    pub fn open(dev: &ID3D11Device, a: &EncodeArgs, adapter_index: u32) -> BoxResult<Self> {
        let mut enc = Self {
            ctx: ptr::null_mut(),
            hw: HwRefs::default(),
            // SAFETY: plain allocations, null-checked below.
            frame: unsafe { ff::av_frame_alloc() },
            qsv_frame: unsafe { ff::av_frame_alloc() },
            pkt: unsafe { ff::av_packet_alloc() },
            feed: Feed::D3d11,
            copy_draw: false,
            path: "d3d11 frames".into(),
        };
        if enc.frame.is_null() || enc.qsv_frame.is_null() || enc.pkt.is_null() {
            return Err("out of memory".into());
        }
        if a.backend == Backend::Nvenc {
            enc.hw.open_d3d11(dev)?;
            // NV12 array textures cannot carry the render-target bind flag (E_INVALIDARG on NVIDIA);
            // pool size 0 makes FFmpeg allocate one single texture per frame instead.
            let pool = if a.format == Format::Bgra {
                POOL_SIZE
            } else {
                0
            };
            enc.hw.open_d3d11_frames(a, BIND_RT, pool)?;
        } else {
            enc.open_qsv(dev, a, adapter_index)?;
        }
        enc.open_codec(a)?;
        Ok(enc)
    }

    fn open_qsv(
        &mut self,
        dev: &ID3D11Device,
        a: &EncodeArgs,
        adapter_index: u32,
    ) -> BoxResult<()> {
        let mut failures = Vec::new();
        for (own_device, bind) in [
            (true, BIND_RT),
            (true, BIND_DECODER),
            (false, BIND_RT),
            (false, BIND_DECODER),
        ] {
            let label = format!(
                "{} device, {} pool",
                if own_device { "our" } else { "ffmpeg-created" },
                if bind == BIND_RT {
                    "render-target"
                } else {
                    "decoder"
                }
            );
            self.hw.release();
            let opened = if own_device {
                self.hw.open_d3d11(dev)
            } else {
                self.hw.open_d3d11_ffmpeg(adapter_index)
            };
            let result = opened
                .and_then(|_| self.hw.open_d3d11_frames(a, bind, POOL_SIZE))
                .and_then(|_| self.hw.derive_qsv());
            match result {
                Ok(()) => {
                    self.feed = Feed::QsvDerived;
                    self.copy_draw = bind != BIND_RT;
                    let copy = if self.copy_draw {
                        ", drawn into own texture + GPU copy"
                    } else {
                        ""
                    };
                    self.path = format!("qsv derived ({label}), frames mapped (direct){copy}");
                    if !failures.is_empty() {
                        self.path
                            .push_str(&format!("; failed before: {}", failures.join("; ")));
                    }
                    return Ok(());
                }
                Err(e) => failures.push(format!("{label}: {e}")),
            }
        }
        self.hw.release();
        Err(format!("no QSV setup opened: {}", failures.join("; ")).into())
    }

    /// The D3D11 device the textures live on (draw with this one).
    pub fn device(&self) -> BoxResult<ID3D11Device> {
        self.hw.d3d11_device()
    }

    /// True when the pool textures cannot be drawn into directly (see `GridPainter` own target).
    pub fn draws_via_copy(&self) -> bool {
        self.copy_draw
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
            check(
                ff::av_hwframe_get_buffer(self.hw.frames, self.frame, 0),
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
