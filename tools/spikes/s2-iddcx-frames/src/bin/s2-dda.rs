//! DDA probe on the virtual monitor (spec 0004, case B). Runs in the console session: started by
//! `s2-svc --mode dda` as SYSTEM, or by hand in a user session for development.
//! Usage: `s2-dda --case B [--minutes 10]`.

use s2_iddcx_frames::d3d::{CellReader, adapters, device_on_adapter, luid_text};
use s2_iddcx_frames::display::{from_wide, virtual_gdi_name};
use s2_iddcx_frames::framelog::{FrameLog, FrameRecord};
use s2_iddcx_frames::frames::{FrameTracker, sample_capacity};
use s2_iddcx_frames::qpc;
use s2_iddcx_frames::report::write_run;
use s2_iddcx_frames::util::{BoxResult, arg_u64, arg_value};
use windows::Win32::Graphics::Direct3D11::{ID3D11Device5, ID3D11Texture2D};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, DXGI_ERROR_ACCESS_LOST, DXGI_ERROR_WAIT_TIMEOUT, DXGI_OUTDUPL_FRAME_INFO,
    IDXGIAdapter1, IDXGIFactory1, IDXGIOutput5, IDXGIOutputDuplication, IDXGIResource,
};
use windows::Win32::System::StationsAndDesktops::{
    DESKTOP_ACCESS_FLAGS, DESKTOP_CONTROL_FLAGS, OpenInputDesktop, SetThreadDesktop,
};
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
};
use windows::core::Interface;

const GENERIC_ALL: DESKTOP_ACCESS_FLAGS = DESKTOP_ACCESS_FLAGS(0x1000_0000);
const RECREATE_WAIT: std::time::Duration = std::time::Duration::from_millis(100);

/// The virtual monitor's output and the adapter it is enumerated on.
fn find_output(gdi_name: &str) -> BoxResult<(IDXGIAdapter1, IDXGIOutput5)> {
    // SAFETY: enumeration stops at DXGI_ERROR_NOT_FOUND.
    unsafe {
        let factory: IDXGIFactory1 = CreateDXGIFactory1()?;
        let mut a = 0;
        while let Ok(adapter) = factory.EnumAdapters1(a) {
            let mut o = 0;
            while let Ok(output) = adapter.EnumOutputs(o) {
                if from_wide(&output.GetDesc()?.DeviceName) == gdi_name {
                    return Ok((adapter, output.cast()?));
                }
                o += 1;
            }
            a += 1;
        }
    }
    Err(format!("no DXGI output named {gdi_name}").into())
}

fn duplicate(output: &IDXGIOutput5, device: &ID3D11Device5) -> BoxResult<IDXGIOutputDuplication> {
    // SAFETY: device is alive; the format list outlives the call.
    Ok(unsafe { output.DuplicateOutput1(device, 0, &[DXGI_FORMAT_B8G8R8A8_UNORM])? })
}

fn main() -> BoxResult<()> {
    let args: Vec<String> = std::env::args().collect();
    let case = arg_value(&args, "--case").unwrap_or("dev").to_string();
    let minutes = arg_u64(&args, "--minutes", 10)?;

    // SAFETY: plain Win32 calls. Failing is fine on the normal desktop of a user session.
    unsafe {
        // DuplicateOutput1 returns DXGI_ERROR_UNSUPPORTED unless the process is per-monitor DPI aware.
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2)?;
        if let Ok(desk) = OpenInputDesktop(DESKTOP_CONTROL_FLAGS(0), false, GENERIC_ALL) {
            let _ = SetThreadDesktop(desk);
        }
    }

    let gdi = virtual_gdi_name().ok_or("virtual monitor not found")?;
    let (adapter, output) = find_output(&gdi).map_err(|e| format!("find_output {gdi}: {e}"))?;
    let (device, ctx) =
        device_on_adapter(&adapter).map_err(|e| format!("device_on_adapter: {e}"))?;
    let reader = CellReader::new(&device, DXGI_FORMAT_B8G8R8A8_UNORM)?;
    let mut dupl = Some(duplicate(&output, &device).map_err(|e| format!("DuplicateOutput1: {e}"))?);
    let mut tracker = FrameTracker::new(sample_capacity(minutes));
    let mut frame_log = FrameLog::new(sample_capacity(minutes));
    let (mut access_lost, mut recreate_failed) = (0u32, 0u32);
    // Access-lost episodes (start, end) in QPC: when duplication was unavailable.
    let mut episodes: Vec<(u64, u64)> = Vec::new();
    let mut lost_since: Option<u64> = None;
    let deadline = qpc::now() + minutes * 60 * qpc::freq();

    while qpc::now() < deadline {
        let Some(d) = &dupl else {
            // Duplication lost (mode change, display off, secure desktop): wait, then recreate.
            std::thread::sleep(RECREATE_WAIT);
            match duplicate(&output, &device) {
                Ok(d) => {
                    if let Some(start) = lost_since.take() {
                        episodes.push((start, qpc::now()));
                    }
                    dupl = Some(d);
                }
                Err(_) => recreate_failed += 1,
            }
            continue;
        };
        let mut info = DXGI_OUTDUPL_FRAME_INFO::default();
        let mut resource: Option<IDXGIResource> = None;
        // SAFETY: out-pointers are valid locals.
        let acquired = unsafe { d.AcquireNextFrame(100, &mut info, &mut resource) };
        let now = qpc::now();
        match acquired {
            Ok(()) => {}
            Err(e) if e.code() == DXGI_ERROR_WAIT_TIMEOUT => continue,
            Err(e) if e.code() == DXGI_ERROR_ACCESS_LOST => {
                access_lost += 1;
                lost_since.get_or_insert(now);
                dupl = None;
                continue;
            }
            Err(e) => return Err(e.into()),
        }
        // HOT PATH: only the counter cell is read back.
        if info.LastPresentTime != 0
            && let Some(tex) = resource.and_then(|r| r.cast::<ID3D11Texture2D>().ok())
        {
            let latency_qpc = now.saturating_sub(info.LastPresentTime as u64);
            let map_start = qpc::now();
            let counter = reader.read(&ctx, &tex);
            let map_qpc = qpc::now() - map_start;
            tracker.on_frame(counter, true, latency_qpc);
            frame_log.push(FrameRecord {
                t_qpc: now,
                counter,
                ok: true,
                generation: access_lost,
                latency_qpc,
                map_qpc,
            });
        }
        // SAFETY: a frame was acquired above.
        unsafe { d.ReleaseFrame()? };
    }
    if let Some(start) = lost_since {
        episodes.push((start, qpc::now()));
    }

    // SAFETY: plain query on a live adapter.
    let luid = unsafe { adapter.GetDesc1()?.AdapterLuid };
    let mut notes = vec![
        format!("output {gdi} on adapter {}", luid_text(luid)),
        format!(
            "access lost {access_lost}, recreate failed {recreate_failed} (retry every 100 ms)"
        ),
    ];
    for (start, end) in &episodes {
        notes.push(format!(
            "no duplication from qpc {start} for {} ms",
            qpc::to_us(end - start) / 1000
        ));
    }
    for (name, luid, mb) in adapters()? {
        notes.push(format!("gpu {name} luid {} vram {mb} MB", luid_text(luid)));
    }
    let delivered = tracker.summary().delivered;
    let path = write_run(
        &case,
        "dda",
        &mut tracker,
        &frame_log,
        [delivered, 0],
        &notes,
    )?;
    eprintln!("s2-dda: {}", path.display());
    Ok(())
}
