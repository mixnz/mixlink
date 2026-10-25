//! The `display ...` developer commands (spec 0015).

use std::io::{BufRead, Write};
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use windows::Win32::System::RemoteDesktop::ProcessIdToSessionId;
use windows::Win32::System::Threading::GetCurrentProcessId;

use mixlink_core::ids::DeviceId;
use mixlink_proto::control::DisplayInfo;

use super::edid::{HEADLESS_ID, HEADLESS_MODE, PRODUCT_HEADLESS};
use super::modes::add_request;
use super::ports::DisplayDriver;
use super::session::{self, Ports};
use super::status::{format_status, refresh_text};
use super::store::SnapshotStore;
use crate::args::{DisplayCmd, TryTopology};
use crate::error::SvcError;
use crate::platform::windows::console::install_ctrl_handler;
use crate::platform::windows::dir_security::secure_dir;
use crate::platform::windows::display_config::{CcdConfig, probe_report};
use crate::platform::windows::drivers_host::program_data;
use crate::platform::windows::gpu_name::gpu_name;
use crate::platform::windows::install::is_running;
use crate::platform::windows::vdisplay::VDisplay;
use crate::stop::StopSignal;
use crate::strings;

pub fn run(cmd: &DisplayCmd) -> Result<(), SvcError> {
    match cmd {
        DisplayCmd::Status => status(),
        DisplayCmd::Try {
            client,
            mode,
            topology,
        } => try_monitor(client, mode, *topology),
        DisplayCmd::Probe { out } => probe(out),
        DisplayCmd::RejectCheck => reject_check(),
    }
}

fn snapshot_store() -> SnapshotStore {
    SnapshotStore::new(SnapshotStore::default_path(&program_data()))
}

fn status() -> Result<(), SvcError> {
    let reply = VDisplay::open()?.query()?;
    let gpu = gpu_name(reply.render_luid);
    let saved = snapshot_store().path().exists();
    let text = format_status(&reply, gpu.as_deref(), saved);
    std::io::stdout().write_all(text.as_bytes())?;
    Ok(())
}

/// The layout file's folder, created and limited to SYSTEM + Administrators (as the driver staging).
fn secured_store() -> Result<SnapshotStore, SvcError> {
    let store = snapshot_store();
    if let Some(dir) = store.path().parent() {
        secure_dir(dir)?;
    }
    Ok(store)
}

/// A session by hand: `begin`, wait for Enter / Ctrl+C, `end`. Refused while the service runs
/// (two owners of the screen layout).
fn try_monitor(
    client: &DeviceId,
    mode: &DisplayInfo,
    topology: TryTopology,
) -> Result<(), SvcError> {
    if is_running()? {
        return Err(SvcError::ServiceRunning);
    }
    let mut driver = VDisplay::open()?;
    let mut config = CcdConfig;
    let store = secured_store()?;
    let mut ports = Ports {
        driver: &mut driver,
        config: &mut config,
        store: &store,
    };
    let topology = match topology {
        TryTopology::Keep => None,
        TryTopology::Apply(topology) => Some(topology),
    };
    let mut headless = None;
    let started = Instant::now();
    let active = session::begin(
        &mut ports,
        client,
        mode,
        topology,
        &mut headless,
        &std::thread::sleep,
    )?;
    let took_ms = started.elapsed().as_millis();
    eprintln!(
        "{}",
        strings::monitor_on(
            mode.width,
            mode.height,
            &refresh_text(mode.refresh_mhz),
            took_ms
        )
    );
    wait_enter_or_ctrl_c()?;
    session::end(&mut ports, active, &mut headless)?;
    eprintln!("{}", strings::MONITOR_OFF);
    Ok(())
}

/// Blocks until the user presses Enter or Ctrl+C.
fn wait_enter_or_ctrl_c() -> Result<(), SvcError> {
    let stop = Arc::new(StopSignal::new());
    install_ctrl_handler(Arc::clone(&stop))?;
    let reader = Arc::clone(&stop);
    std::thread::Builder::new()
        .name("display-try-stdin".into())
        .spawn(move || {
            let mut line = String::new();
            let _ = std::io::stdin().lock().read_line(&mut line);
            reader.stop();
        })?;
    stop.wait();
    Ok(())
}

/// Writes the session id and the layout read / re-apply results to `out` (spec 0015, Session 0 check).
fn probe(out: &Path) -> Result<(), SvcError> {
    let mut session = 0u32;
    // SAFETY: our own process id; `session` is a valid out pointer.
    let session_text =
        match unsafe { ProcessIdToSessionId(GetCurrentProcessId(), &raw mut session) } {
            Ok(()) => session.to_string(),
            Err(error) => error.to_string(),
        };
    let text = format!("session: {session_text}\n{}", probe_report());
    std::fs::write(out, text)?;
    Ok(())
}

/// Malformed `ADD_MONITOR` requests the driver must refuse (spec 0015 acceptance).
fn reject_check() -> Result<(), SvcError> {
    let mut driver = VDisplay::open()?;
    let valid = add_request(&HEADLESS_ID, PRODUCT_HEADLESS, &HEADLESS_MODE).encode()?;
    let cases: [(&str, usize, u32); 5] = [
        (
            "bad EDID checksum",
            36 + 127,
            u32::from(valid[36 + 127] ^ 0xFF),
        ),
        ("0 modes", 28, 0),
        ("17 modes", 28, 17),
        ("width 7681", 548, 7681),
        ("flags = 1", 4, 1),
    ];
    let mut failed = Vec::new();
    for (name, at, value) in cases {
        let mut input = valid;
        if name == "bad EDID checksum" {
            input[at] = u8::try_from(value).unwrap_or(0);
        } else {
            input[at..at + 4].copy_from_slice(&value.to_le_bytes());
        }
        let refused = matches!(
            driver.add_raw(&input),
            Err(SvcError::Win32 { ref source, .. })
                if source.code() == windows::Win32::Foundation::ERROR_INVALID_PARAMETER.to_hresult()
        );
        eprintln!(
            "{name}: {}",
            if refused { "refused" } else { "NOT refused" }
        );
        if !refused {
            failed.push(name);
        }
    }
    let left = driver.query()?.monitors.len();
    eprintln!("monitors after the check: {left}");
    if failed.is_empty() && left == 0 {
        Ok(())
    } else {
        Err(SvcError::Driver {
            step: "reject-check",
            detail: format!("accepted: {failed:?}, monitors left: {left}"),
        })
    }
}
