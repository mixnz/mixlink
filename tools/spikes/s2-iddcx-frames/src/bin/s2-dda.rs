//! DDA probe on the virtual monitor (spec 0004, case B). Runs in the console session: started by
//! `s2-svc --mode dda` as SYSTEM, or by hand in a user session for development.
//! Usage: `s2-dda --case B [--minutes 10]`.

use s2_iddcx_frames::d3d::{CellReader, adapters, device_on_adapter, luid_text};
use s2_iddcx_frames::display::{from_wide, virtual_gdi_name};
use s2_iddcx_frames::frames::FrameTracker;
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
use windows::core::Interface;

const GENERIC_ALL: DESKTOP_ACCESS_FLAGS = DESKTOP_ACCESS_FLAGS(0x1000_0000);

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
        if let Ok(desk) = OpenInputDesktop(DESKTOP_CONTROL_FLAGS(0), false, GENERIC_ALL) {
            let _ = SetThreadDesktop(desk);
        }
    }

    let gdi = virtual_gdi_name().ok_or("virtual monitor not found")?;
    let (adapter, output) = find_output(&gdi)?;
    let (device, ctx) = device_on_adapter(&adapter)?;
    let reader = CellReader::new(&device, DXGI_FORMAT_B8G8R8A8_UNORM)?;
    let mut dupl = duplicate(&output, &device)?;
    let mut tracker = FrameTracker::new((minutes * 60 * 130) as usize);
    let mut access_lost = 0u32;
    let deadline = qpc::now() + minutes * 60 * qpc::freq();

    while qpc::now() < deadline {
        let mut info = DXGI_OUTDUPL_FRAME_INFO::default();
        let mut resource: Option<IDXGIResource> = None;
        // SAFETY: out-pointers are valid locals.
        let acquired = unsafe { dupl.AcquireNextFrame(100, &mut info, &mut resource) };
        let now = qpc::now();
        match acquired {
            Ok(()) => {}
            Err(e) if e.code() == DXGI_ERROR_WAIT_TIMEOUT => continue,
            Err(e) if e.code() == DXGI_ERROR_ACCESS_LOST => {
                access_lost += 1;
                dupl = duplicate(&output, &device)?;
                continue;
            }
            Err(e) => return Err(e.into()),
        }
        // HOT PATH: only the counter cell is read back.
        if info.LastPresentTime != 0
            && let Some(tex) = resource.and_then(|r| r.cast::<ID3D11Texture2D>().ok())
        {
            let latency = now.saturating_sub(info.LastPresentTime as u64);
            tracker.on_frame(reader.read(&ctx, &tex), true, latency);
        }
        // SAFETY: a frame was acquired above.
        unsafe { dupl.ReleaseFrame()? };
    }

    // SAFETY: plain query on a live adapter.
    let luid = unsafe { adapter.GetDesc1()?.AdapterLuid };
    let mut notes = vec![
        format!("output {gdi} on adapter {}", luid_text(luid)),
        format!("access lost {access_lost}"),
    ];
    for (name, luid, mb) in adapters()? {
        notes.push(format!("gpu {name} luid {} vram {mb} MB", luid_text(luid)));
    }
    let delivered = tracker.summary().delivered;
    let path = write_run(&case, "dda", &mut tracker, [delivered, 0], &notes)?;
    eprintln!("s2-dda: {}", path.display());
    Ok(())
}
