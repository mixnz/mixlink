//! The `display ...` developer commands (spec 0015).

use std::io::{BufRead, Write};
use std::sync::Arc;

use mixlink_core::ids::DeviceId;
use mixlink_proto::control::DisplayInfo;

use super::edid::{HEADLESS_ID, HEADLESS_MODE, PRODUCT_CLIENT, PRODUCT_HEADLESS};
use super::modes::add_request;
use super::ports::DisplayDriver;
use super::status::{format_status, refresh_text};
use crate::args::{DisplayCmd, TryTopology};
use crate::error::SvcError;
use crate::platform::windows::console::install_ctrl_handler;
use crate::platform::windows::gpu_name::gpu_name;
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
        DisplayCmd::RejectCheck => reject_check(),
    }
}

fn status() -> Result<(), SvcError> {
    let reply = VDisplay::open()?.query()?;
    let gpu = gpu_name(reply.render_luid);
    let text = format_status(&reply, gpu.as_deref(), false);
    std::io::stdout().write_all(text.as_bytes())?;
    Ok(())
}

fn try_monitor(
    client: &DeviceId,
    mode: &DisplayInfo,
    topology: TryTopology,
) -> Result<(), SvcError> {
    let mut driver = VDisplay::open()?;
    let TryTopology::Keep = topology;
    let id = driver.add_monitor(&add_request(client, PRODUCT_CLIENT, mode))?;
    eprintln!(
        "{}",
        strings::monitor_on(mode.width, mode.height, &refresh_text(mode.refresh_mhz))
    );
    wait_enter_or_ctrl_c()?;
    driver.remove_monitor(id)?;
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
