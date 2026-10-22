//! `s2-svc` frame loop: waits on the driver's fence, measures latency, or saves PNGs.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use windows::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT;
use windows::Win32::System::Threading::{CreateEventW, WaitForSingleObject};
use windows::core::PCWSTR;

use crate::d3d::{CellReader, adapters, luid_text, save_png};
use crate::diag::Diag;
use crate::framelog::{FrameLog, FrameRecord};
use crate::frames::{FrameTracker, RefKind, reference_qpc, sample_capacity};
use crate::header::{GenerationWatch, SLOT_COUNT, validate};
use crate::log::RunLog;
use crate::qpc;
use crate::report::write_run;
use crate::shared::{Section, SharedFrames};
use crate::util::{BoxResult, now_stamp, out_dir};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Timing,
    Png,
}

const RETRY: Duration = Duration::from_millis(500);
const PNG_EVERY_S: u64 = 5;

fn open_section(stop: &AtomicBool, deadline: u64, log: &mut RunLog) -> Option<Section> {
    let mut logged = false;
    while !stop.load(Ordering::Relaxed) && qpc::now() < deadline {
        match Section::open() {
            Ok(s) => return Some(s),
            Err(e) if !logged => {
                log.line(&format!("waiting for the driver's section: {e}"));
                logged = true;
            }
            Err(_) => {}
        }
        std::thread::sleep(RETRY);
    }
    None
}

pub fn run(
    mode: Mode,
    case: &str,
    minutes: u64,
    stop: &AtomicBool,
    log: &mut RunLog,
) -> BoxResult<()> {
    let deadline = qpc::now() + minutes * 60 * qpc::freq();
    let Some(section) = open_section(stop, deadline, log) else {
        log.line("stopped before the section appeared");
        return Ok(());
    };
    log.line("section open");
    // SAFETY: unnamed auto-reset event, closed at the end.
    let event = unsafe { CreateEventW(None, false, false, PCWSTR::null())? };
    let mut watch = GenerationWatch::default();
    let mut frames: Option<(SharedFrames, CellReader)> = None;
    let mut tracker = FrameTracker::new(sample_capacity(minutes));
    let mut frame_log = FrameLog::new(sample_capacity(minutes));
    let (mut ref_counts, mut diag) = ([0u64; 2], Diag::default());
    let (mut next, mut last_png, mut opens, mut pngs) = (0u64, 0u64, 0u32, 0u32);
    let mut notes = Vec::new();

    while !stop.load(Ordering::Relaxed) && qpc::now() < deadline {
        let h = section.header();
        if validate(&h).is_err() {
            frames = None;
            std::thread::sleep(RETRY);
            continue;
        }
        if watch.changed(h.generation) || frames.is_none() {
            frames = None;
            let opened = SharedFrames::open(&h).and_then(|f| {
                let reader = CellReader::new(&f.device, DXGI_FORMAT(h.dxgi_format as i32))?;
                Ok((f, reader))
            });
            match opened {
                Ok(pair) => {
                    opens += 1;
                    let line = format!(
                        "opened generation {} via DuplicateHandle from WUDFHost pid {}: {}x{} format {} luid {}",
                        h.generation,
                        h.wudf_pid,
                        h.width,
                        h.height,
                        h.dxgi_format,
                        luid_text(pair.0.luid)
                    );
                    log.line(&line);
                    notes.push(line);
                    next = h.latest_seq + 1;
                    frames = Some(pair);
                }
                Err(e) => {
                    log.line(&format!("open generation {} failed: {e}", h.generation));
                    std::thread::sleep(RETRY);
                    continue;
                }
            }
        }
        let Some((f, reader)) = &frames else { continue };

        // SAFETY: fence and event are alive.
        let waited = unsafe {
            f.fence.SetEventOnCompletion(next, event)?;
            WaitForSingleObject(event, 100)
        };
        let now = qpc::now();
        if waited != WAIT_OBJECT_0 {
            // Static screen (lock, login): DWM stops presenting, keep saving the newest frame.
            let latest = section.latest_seq();
            if mode == Mode::Png && latest > 0 && now - last_png >= PNG_EVERY_S * qpc::freq() {
                save(
                    f,
                    (latest % SLOT_COUNT as u64) as usize,
                    case,
                    &mut pngs,
                    log,
                );
                last_png = now;
            }
            continue;
        }

        // HOT PATH (timing mode): no allocation, only the counter cell is read back.
        let latest = section.latest_seq();
        if latest >= next + SLOT_COUNT as u64 - 1 {
            next = latest; // older slots are already overwritten; the counter gap counts them
        }
        let idx = (next % SLOT_COUNT as u64) as usize;
        let slot = section.slot(idx);
        match mode {
            Mode::Timing => {
                let map_start = qpc::now();
                let counter = if slot.seq == next {
                    reader.read(&f.ctx, &f.ring[idx])
                } else {
                    None
                };
                let map_qpc = qpc::now() - map_start;
                let ok = slot.seq == next && section.slot_seq(idx) == next;
                let (reference, kind) = reference_qpc(slot.present_qpc, slot.acquire_qpc);
                if ok {
                    ref_counts[match kind {
                        RefKind::Present => 0,
                        RefKind::Acquire => 1,
                    }] += 1;
                    diag.add(&slot, now);
                }
                let latency_qpc = now.saturating_sub(reference);
                tracker.on_frame(counter, ok, latency_qpc);
                frame_log.push(FrameRecord {
                    t_qpc: now,
                    counter,
                    ok,
                    generation: h.generation,
                    latency_qpc,
                    map_qpc,
                });
            }
            Mode::Png => {
                if now - last_png >= PNG_EVERY_S * qpc::freq() {
                    save(f, idx, case, &mut pngs, log);
                    last_png = now;
                }
            }
        }
        next += 1;
    }

    // SAFETY: created above.
    unsafe {
        let _ = CloseHandle(event);
    }
    log.line(&format!("loop ended: opens {opens}, pngs {pngs}"));
    if mode == Mode::Timing {
        notes.push(format!("generations opened {opens}"));
        notes.extend(diag.notes());
        for (name, luid, mb) in adapters()? {
            notes.push(format!("gpu {name} luid {} vram {mb} MB", luid_text(luid)));
        }
        let path = write_run(case, "iddcx", &mut tracker, &frame_log, ref_counts, &notes)?;
        log.line(&format!("summary {}", path.display()));
    }
    Ok(())
}

fn save(f: &SharedFrames, idx: usize, case: &str, pngs: &mut u32, log: &mut RunLog) {
    let result = out_dir().and_then(|dir| {
        let path = dir.join(format!("{}-{case}.png", now_stamp()));
        save_png(&f.device, &f.ctx, &f.ring[idx], &path)?;
        Ok(path)
    });
    match result {
        Ok(path) => {
            *pngs += 1;
            log.line(&format!("png {}", path.display()));
        }
        Err(e) => log.line(&format!("png failed: {e}")),
    }
}
