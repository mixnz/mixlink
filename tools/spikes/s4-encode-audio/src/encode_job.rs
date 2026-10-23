//! Encode job: draw the frame-number grid, wait for the GPU, time `send_frame` → packet with that pts.

use std::fmt::Write as _;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::args::{Backend, EncodeArgs};
use crate::log::RunLog;
use ffmpeg_sys_next as ff;

use crate::platform::d3d::{
    GridPainter, VENDOR_INTEL, VENDOR_NVIDIA, adapter_index, device_for_vendor, immediate_context,
};
use crate::platform::ff_encode::HwEncoder;
use crate::platform::ff_util;
use crate::platform::pacer::Pacer;
use crate::qpc;
use crate::stats::percentiles;
use crate::util::{BoxResult, RunFiles};

/// Packets with pts below this go into the bitstream dump (validity check).
pub const DUMP_FRAMES: i64 = 600;
const DUMP_CAPACITY: usize = 64 * 1024 * 1024;
/// A frame whose packet has not come back after this long is given up on (counted lost unless it
/// still arrives before the flush).
const FRAME_TIMEOUT_MS: u64 = 100;

/// Takes the send time stored for `pts` and returns the latency in QPC ticks; `None` for a pts that
/// was never sent or was already matched. Packets are matched by pts, never by arrival order.
pub fn record(pending: &mut [Option<u64>], pts: i64, t1: u64) -> Option<u64> {
    let slot = pending.get_mut(usize::try_from(pts).ok()?)?;
    slot.take().map(|t0| t1.saturating_sub(t0))
}

/// Frames still waiting for their packet.
pub fn count_lost(pending: &[Option<u64>]) -> u32 {
    pending.iter().filter(|p| p.is_some()).count() as u32
}

pub struct EncodeResult {
    pub summary: String,
    pub latency_us: Vec<u64>,
}

pub fn run(
    a: &EncodeArgs,
    files: &RunFiles,
    stop: &AtomicBool,
    log: &mut RunLog,
) -> BoxResult<EncodeResult> {
    let vendor = match a.backend {
        Backend::Nvenc => VENDOR_NVIDIA,
        Backend::Qsv => VENDOR_INTEL,
    };
    let (own_device, _, adapter) = device_for_vendor(vendor, a.backend == Backend::Qsv)?;
    log.line(&format!(
        "adapter {adapter}, encoder {}, format {:?}",
        a.encoder_name(),
        a.format
    ));
    // FFmpeg's own log lines (a service has no stderr): verbose while opening, warnings afterwards.
    ff_util::log_to_file(&files.path("ffmpeg.log"), ff::AV_LOG_VERBOSE)?;
    let opened = HwEncoder::open(&own_device, a, adapter_index(vendor)?);
    ff_util::set_log_level(ff::AV_LOG_WARNING);
    let mut enc = opened?;
    // Draw on the device the pool textures live on (ours, or one FFmpeg created for QSV).
    let device = enc.device()?;
    let ctx = immediate_context(&device)?;
    let mut painter = GridPainter::new(
        &device,
        a.format,
        enc.draws_via_copy().then_some((a.width, a.height)),
    )?;
    let pacer = Pacer::new()?;
    log.line(&format!("encoder open, path: {}", enc.path));

    let n = a.frames as usize;
    let mut pending: Vec<Option<u64>> = vec![None; n];
    let mut latency_ticks: Vec<u64> = Vec::with_capacity(n);
    let mut dump: Vec<u8> = Vec::with_capacity(DUMP_CAPACITY);
    let mut bytes = 0u64;
    let period = qpc::freq() / u64::from(a.fps);
    let timeout = qpc::freq() * FRAME_TIMEOUT_MS / 1_000;
    let start = qpc::now() + period;
    let mut sent = 0usize;

    // HOT PATH: per frame; buffers above are preallocated.
    let mut sink = |pts: i64, data: &[u8], pending: &mut [Option<u64>]| {
        if let Some(ticks) = record(pending, pts, qpc::now()) {
            latency_ticks.push(ticks);
        }
        bytes += data.len() as u64;
        if pts < DUMP_FRAMES && dump.len() + data.len() <= dump.capacity() {
            dump.extend_from_slice(data);
        }
    };
    for i in 0..n {
        if stop.load(Ordering::Acquire) {
            break;
        }
        pacer.wait_until(start + i as u64 * period);
        let (tex, slice) = enc.next_frame()?;
        painter.draw(&ctx, &tex, slice, i as u32)?;
        painter.wait_idle(&ctx);
        let t0 = qpc::now();
        pending[i] = Some(t0);
        enc.send(i as i64)?;
        sent = i + 1;
        while pending[i].is_some() && qpc::now() - t0 < timeout {
            if !enc.poll(&mut |pts, data| sink(pts, data, &mut pending))? {
                std::hint::spin_loop();
            }
        }
    }
    enc.flush(&mut |pts, data| sink(pts, data, &mut pending))?;
    let seconds = (qpc::now() - start) as f64 / qpc::freq() as f64;
    let lost = count_lost(&pending[..sent]);

    let latency_us: Vec<u64> = latency_ticks.iter().map(|&t| qpc::to_us(t)).collect();
    std::fs::write(files.path(&format!("encode.{}", a.codec.ext())), &dump)?;
    let s = percentiles(&mut latency_us.clone());
    let mut summary = String::new();
    let _ = writeln!(
        summary,
        "case {} side {} job encode",
        files.case, files.side
    );
    let _ = writeln!(summary, "adapter: {adapter}");
    let _ = writeln!(
        summary,
        "encoder: {} format {:?} {}x{}@{} {} bit/s",
        a.encoder_name(),
        a.format,
        a.width,
        a.height,
        a.fps,
        a.bitrate
    );
    let _ = writeln!(summary, "path: {}", enc.path);
    let _ = writeln!(
        summary,
        "frames sent {sent} received {} lost {lost}",
        latency_us.len()
    );
    let _ = writeln!(
        summary,
        "latency us: p50 {} p99 {} max {}",
        s.p50, s.p99, s.max
    );
    let _ = writeln!(
        summary,
        "bitrate: {:.1} Mbit/s over {seconds:.1} s",
        bytes as f64 * 8.0 / seconds.max(0.001) / 1e6
    );
    Ok(EncodeResult {
        summary,
        latency_us,
    })
}

#[cfg(test)]
#[path = "encode_job_tests.rs"]
mod tests;
