//! Audio job: loopback of the default render endpoint for N minutes; packet interval, offset of the
//! device QPC position from now (signed), discontinuities, loud/silent seconds, reopen gap on device change.

use std::fmt::Write as _;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use windows::Win32::System::Threading::AvSetMmThreadCharacteristicsW;
use windows::core::w;

use crate::args::AudioArgs;
use crate::log::RunLog;
use crate::platform::notify::watch_default_render;
use crate::platform::tone;
use crate::platform::wasapi::Loopback;
use crate::qpc;
use crate::stats::percentiles;
use crate::util::{BoxResult, RunFiles};

/// RMS floor (digital silence).
const FLOOR_DBFS: f64 = -120.0;
/// A second is "loud" (signal present) above this.
pub const LOUD_DBFS: f64 = -50.0;
const FLAG_DISCONTINUITY: u32 = 1;
const FLAG_TIMESTAMP_ERROR: u32 = 4;

static DEFAULT_CHANGED: AtomicBool = AtomicBool::new(false);

/// RMS of `sum_sq` over `n` samples in dBFS, clamped at -120.
pub fn rms_dbfs(sum_sq: f64, n: u64) -> f64 {
    if n == 0 || sum_sq <= 0.0 {
        return FLOOR_DBFS;
    }
    (20.0 * (sum_sq / n as f64).sqrt().log10()).max(FLOOR_DBFS)
}

pub fn is_loud(dbfs: f64) -> bool {
    dbfs > LOUD_DBFS
}

/// Nearest-rank (p1, p50, p99) of signed samples; sorts in place. Zeros when empty.
pub fn signed_percentiles(v: &mut [i64]) -> (i64, i64, i64) {
    if v.is_empty() {
        return (0, 0, 0);
    }
    v.sort_unstable();
    let rank = |p: usize| v[(p * v.len()).div_ceil(100).max(1) - 1];
    (rank(1), rank(50), rank(99))
}

pub struct AudioResult {
    pub summary: String,
    /// QPC now - device QPC position per packet, µs (negative = position ahead of now).
    pub offset_us: Vec<i64>,
    pub interval_us: Vec<u64>,
}

#[derive(Default)]
struct Counters {
    discontinuities: u32,
    timestamp_errors: u32,
    loud_s: u32,
    quiet_s: u32,
    open_errors: u32,
    reopen_gaps_ms: Vec<u64>,
}

/// Opens loopback, retrying every second until it works or `deadline` passes (no endpoint at the
/// login screen, device switching).
fn open_retry(
    deadline: u64,
    stop: &AtomicBool,
    c: &mut Counters,
    log: &mut RunLog,
) -> Option<Loopback> {
    while qpc::now() < deadline && !stop.load(Ordering::Acquire) {
        match Loopback::open_default() {
            Ok(lb) => {
                log.line(&format!("loopback open: {}", lb.info));
                return Some(lb);
            }
            Err(e) => {
                c.open_errors += 1;
                log.line(&format!("loopback open failed: {e}"));
                std::thread::sleep(Duration::from_secs(1));
            }
        }
    }
    None
}

pub fn run(
    a: &AudioArgs,
    files: &RunFiles,
    stop: &AtomicBool,
    log: &mut RunLog,
) -> BoxResult<AudioResult> {
    let mut task = 0u32;
    // SAFETY: registers this thread with MMCSS; the handle lives until the thread ends.
    if let Err(e) = unsafe { AvSetMmThreadCharacteristicsW(w!("Pro Audio"), &mut task) } {
        log.line(&format!("MMCSS Pro Audio failed: {e}"));
    }
    let _watch = watch_default_render(&DEFAULT_CHANGED)?;
    let stop_tone = AtomicBool::new(false);
    std::thread::scope(|s| {
        let tone = a
            .tone
            .then(|| s.spawn(|| tone::play(&stop_tone).map_err(|e| e.to_string())));
        let result = capture(a, files, stop, log);
        stop_tone.store(true, Ordering::Release);
        if let Some(Ok(Err(e))) = tone.map(|t| t.join()) {
            log.line(&format!("tone failed: {e}"));
        }
        result
    })
}

fn capture(
    a: &AudioArgs,
    files: &RunFiles,
    stop: &AtomicBool,
    log: &mut RunLog,
) -> BoxResult<AudioResult> {
    let freq = qpc::freq();
    let start = qpc::now();
    let deadline = start + u64::from(a.minutes) * 60 * freq;
    // Up to one packet per ms (1 ms is below any shared-mode engine period).
    let capacity = a.minutes as usize * 60_000;
    let mut offset_us: Vec<i64> = Vec::with_capacity(capacity);
    let mut interval_us = Vec::with_capacity(capacity);
    let mut c = Counters::default();
    let mut infos = Vec::new();
    let Some(mut lb) = open_retry(deadline, stop, &mut c, log) else {
        return Err("loopback never opened".into());
    };
    infos.push(lb.info.clone());
    let (mut sum_sq, mut window_frames, mut last) = (0.0f64, 0u64, 0u64);
    // The first packet after an open always carries the discontinuity flag; it is not counted.
    let mut fresh = true;
    let mut change_at: Option<u64> = None;
    while qpc::now() < deadline && !stop.load(Ordering::Acquire) {
        if DEFAULT_CHANGED.swap(false, Ordering::AcqRel) {
            drop(lb);
            let t = qpc::now();
            log.line("default render endpoint changed; reopening");
            let Some(next) = open_retry(deadline, stop, &mut c, log) else {
                break;
            };
            lb = next;
            infos.push(lb.info.clone());
            change_at = Some(t);
            (last, fresh) = (0, true);
        }
        if !lb.wait(200) {
            continue;
        }
        let (rate, channels) = (u64::from(lb.rate), u64::from(lb.channels));
        // HOT PATH: per packet; vectors preallocated above.
        lb.read(&mut |samples, frames, flags, qpc_100ns| {
            let now = qpc::now();
            if let Some(t) = change_at.take() {
                c.reopen_gaps_ms.push(qpc::to_us(now - t) / 1_000);
            }
            if last != 0 {
                interval_us.push(qpc::to_us(now - last));
            }
            last = now;
            if flags & FLAG_TIMESTAMP_ERROR != 0 {
                c.timestamp_errors += 1;
            } else {
                let now_100ns = (u128::from(now) * 10_000_000 / u128::from(freq)) as u64;
                offset_us.push((now_100ns as i64 - qpc_100ns as i64) / 10);
            }
            if flags & FLAG_DISCONTINUITY != 0 && !fresh {
                c.discontinuities += 1;
            }
            fresh = false;
            sum_sq += samples
                .iter()
                .map(|&v| f64::from(v) * f64::from(v))
                .sum::<f64>();
            window_frames += u64::from(frames);
            if window_frames >= rate {
                if is_loud(rms_dbfs(sum_sq, window_frames * channels)) {
                    c.loud_s += 1;
                } else {
                    c.quiet_s += 1;
                }
                (sum_sq, window_frames) = (0.0, 0);
            }
        })?;
    }
    let seconds = (qpc::now() - start) as f64 / freq as f64;
    Ok(AudioResult {
        summary: summary(files, &infos, &c, &offset_us, &interval_us, seconds),
        offset_us,
        interval_us,
    })
}

fn summary(
    files: &RunFiles,
    infos: &[String],
    c: &Counters,
    off: &[i64],
    int: &[u64],
    seconds: f64,
) -> String {
    let (o, i) = (
        signed_percentiles(&mut off.to_vec()),
        percentiles(&mut int.to_vec()),
    );
    let mut s = String::new();
    let _ = writeln!(
        s,
        "case {} side {} job audio ({seconds:.0} s)",
        files.case, files.side
    );
    for info in infos {
        let _ = writeln!(s, "open: {info}");
    }
    let _ = writeln!(
        s,
        "packets {} interval us: p50 {} p99 {} max {}",
        i.count + 1,
        i.p50,
        i.p99,
        i.max
    );
    let _ = writeln!(
        s,
        "now - device qpc position us: p1 {} p50 {} p99 {}",
        o.0, o.1, o.2
    );
    let _ = writeln!(
        s,
        "discontinuities {} timestamp errors {}",
        c.discontinuities, c.timestamp_errors
    );
    let _ = writeln!(
        s,
        "seconds loud {} quiet {} (loud = RMS > {LOUD_DBFS} dBFS)",
        c.loud_s, c.quiet_s
    );
    let _ = writeln!(
        s,
        "open errors {} reopen gaps ms {:?}",
        c.open_errors, c.reopen_gaps_ms
    );
    s
}

#[cfg(test)]
#[path = "audio_job_tests.rs"]
mod tests;
