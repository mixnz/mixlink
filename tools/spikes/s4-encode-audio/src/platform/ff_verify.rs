//! Bitstream check: software-decode a dump, count frames and errors, read the frame-number grid back.

use std::ffi::CString;
use std::path::Path;
use std::ptr;

use ffmpeg_sys_next as ff;

use crate::grid;
use crate::platform::ff_util::{EAGAIN, check};
use crate::util::BoxResult;

/// Frame indices whose grid is read back.
pub const PROBE_FRAMES: [u32; 3] = [0, 299, 599];

pub struct Verified {
    pub frames: u32,
    pub errors: u32,
    /// (frame index, number read from its grid).
    pub numbers: Vec<(u32, u32)>,
}

struct Decoder {
    ctx: *mut ff::AVCodecContext,
    parser: *mut ff::AVCodecParserContext,
    pkt: *mut ff::AVPacket,
    frame: *mut ff::AVFrame,
}

impl Drop for Decoder {
    fn drop(&mut self) {
        // SAFETY: all owned here; free functions accept null.
        unsafe {
            ff::av_parser_close(self.parser);
            ff::avcodec_free_context(&mut self.ctx);
            ff::av_packet_free(&mut self.pkt);
            ff::av_frame_free(&mut self.frame);
        }
    }
}

impl Decoder {
    fn open(name: &str) -> BoxResult<Self> {
        let cname = CString::new(name)?;
        // SAFETY: allocations null-checked; the codec pointer is static.
        unsafe {
            let codec = ff::avcodec_find_decoder_by_name(cname.as_ptr());
            if codec.is_null() {
                return Err(format!("decoder {name} not in this FFmpeg build").into());
            }
            let dec = Self {
                ctx: ff::avcodec_alloc_context3(codec),
                parser: ff::av_parser_init((*codec).id as i32),
                pkt: ff::av_packet_alloc(),
                frame: ff::av_frame_alloc(),
            };
            if dec.ctx.is_null() || dec.parser.is_null() || dec.pkt.is_null() || dec.frame.is_null()
            {
                return Err("decoder allocation failed".into());
            }
            check(
                ff::avcodec_open2(dec.ctx, codec, ptr::null_mut()),
                "avcodec_open2 (decoder)",
            )?;
            Ok(dec)
        }
    }

    /// Sends one packet (null = flush) and drains every frame it releases.
    fn decode(&mut self, pkt: *const ff::AVPacket, out: &mut Verified) {
        // SAFETY: context, frame live; luma plane valid for height x linesize while the frame is held.
        unsafe {
            if ff::avcodec_send_packet(self.ctx, pkt) < 0 {
                out.errors += 1;
            }
            loop {
                let ret = ff::avcodec_receive_frame(self.ctx, self.frame);
                if ret == EAGAIN || ret == ff::AVERROR_EOF {
                    return;
                }
                if ret < 0 {
                    out.errors += 1;
                    return;
                }
                let f = &*self.frame;
                if f.decode_error_flags != 0 {
                    out.errors += 1;
                }
                if PROBE_FRAMES.contains(&out.frames) {
                    let stride = f.linesize[0] as usize;
                    let luma = std::slice::from_raw_parts(f.data[0], stride * f.height as usize);
                    out.numbers.push((out.frames, grid::read(luma, stride)));
                }
                out.frames += 1;
                ff::av_frame_unref(self.frame);
            }
        }
    }
}

/// Decodes a raw `.h264` / `.hevc` Annex B file with FFmpeg's software decoder.
pub fn verify(path: &Path) -> BoxResult<Verified> {
    let name = match path.extension().and_then(|e| e.to_str()) {
        Some("h264") => "h264",
        Some("hevc") => "hevc",
        _ => return Err("file must end in .h264 or .hevc".into()),
    };
    let data = std::fs::read(path)?;
    let mut dec = Decoder::open(name)?;
    let mut out = Verified {
        frames: 0,
        errors: 0,
        numbers: Vec::new(),
    };
    let mut rest: &[u8] = &data;
    // Once the input is used up, the parser is called with an empty buffer until it hands back its
    // last buffered frame (size 0 = fully drained).
    let mut draining = false;
    loop {
        let (mut buf, mut size) = (ptr::null_mut(), 0);
        // SAFETY: parser and context live; input slice valid; output points into parser memory.
        let used = unsafe {
            ff::av_parser_parse2(
                dec.parser,
                dec.ctx,
                &mut buf,
                &mut size,
                rest.as_ptr(),
                rest.len() as i32,
                ff::AV_NOPTS_VALUE,
                ff::AV_NOPTS_VALUE,
                0,
            )
        };
        check(used, "av_parser_parse2")?;
        rest = &rest[used as usize..];
        if size > 0 {
            // SAFETY: the packet borrows the parser's buffer only for this decode call.
            unsafe {
                (*dec.pkt).data = buf;
                (*dec.pkt).size = size;
            }
            let pkt = dec.pkt;
            dec.decode(pkt, &mut out);
        }
        if draining && size == 0 {
            break;
        }
        draining = rest.is_empty();
    }
    dec.decode(ptr::null(), &mut out);
    Ok(out)
}
