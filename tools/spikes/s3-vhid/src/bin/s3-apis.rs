//! s3-apis: which input APIs see which device (spec 0006 case B). Changes nothing.

use std::process::ExitCode;

use s3_vhid::log;
use s3_vhid::platform::{devices, dinput};
use windows::Gaming::Input::{Gamepad, RawGameController};
use windows::Win32::UI::Input::XboxController::{XINPUT_STATE, XInputGetState};

fn main() -> ExitCode {
    let mut lines = vec!["s3-apis".to_string(), "api | device | detail".to_string()];
    raw_input(&mut lines);
    direct_input(&mut lines);
    wgi(&mut lines);
    xinput(&mut lines);
    for line in &lines {
        println!("{line}");
    }
    match log::write_text(&log::run_name("apis"), &lines) {
        Ok(path) => println!("Log: {}", path.display()),
        Err(e) => eprintln!("log not written: {e}"),
    }
    ExitCode::SUCCESS
}

fn raw_input(lines: &mut Vec<String>) {
    match devices::list() {
        Ok(list) => {
            let ours: Vec<_> = list.iter().filter(|d| d.is_vhid()).collect();
            if ours.is_empty() {
                lines.push("Raw Input | (none of ours) | vhid not installed?".into());
            }
            for d in ours {
                lines.push(format!(
                    "Raw Input | vhid Col{:02} | {} usage {:#06x}/{:#04x}",
                    d.collection().unwrap_or(0),
                    d.kind,
                    d.usage_page,
                    d.usage
                ));
            }
        }
        Err(e) => lines.push(format!("Raw Input | ERROR | {e}")),
    }
}

fn direct_input(lines: &mut Vec<String>) {
    match dinput::game_controllers() {
        Ok(list) if list.is_empty() => lines.push("DirectInput | (no game controllers) |".into()),
        Ok(list) => {
            for d in list {
                lines.push(format!(
                    "DirectInput | {} | VID {:04X} PID {:04X}",
                    d.name, d.vid, d.pid
                ));
            }
        }
        Err(e) => lines.push(format!("DirectInput | ERROR | {e}")),
    }
}

fn wgi(lines: &mut Vec<String>) {
    // WGI starts enumerating on first access and fills its lists asynchronously:
    // touch both lists, wait, then read them.
    let _ = RawGameController::RawGameControllers();
    let _ = Gamepad::Gamepads();
    std::thread::sleep(std::time::Duration::from_millis(1500));
    match RawGameController::RawGameControllers() {
        Ok(list) => {
            for i in 0..list.Size().unwrap_or(0) {
                if let Ok(c) = list.GetAt(i) {
                    let name = c.DisplayName().map(|n| n.to_string()).unwrap_or_default();
                    lines.push(format!(
                        "WGI RawGameController | {name} | VID {:04X} PID {:04X}, axes {}, buttons {}, switches {}",
                        c.HardwareVendorId().unwrap_or(0), c.HardwareProductId().unwrap_or(0),
                        c.AxisCount().unwrap_or(-1), c.ButtonCount().unwrap_or(-1), c.SwitchCount().unwrap_or(-1)
                    ));
                }
            }
        }
        Err(e) => lines.push(format!("WGI RawGameController | ERROR | {e}")),
    }
    match Gamepad::Gamepads().and_then(|g| g.Size()) {
        Ok(n) => lines.push(format!("WGI Gamepad | count {n} | Xbox-class pads only")),
        Err(e) => lines.push(format!("WGI Gamepad | ERROR | {e}")),
    }
}

fn xinput(lines: &mut Vec<String>) {
    for index in 0..4u32 {
        let mut state = XINPUT_STATE::default();
        // SAFETY: valid out pointer.
        let connected = unsafe { XInputGetState(index, &mut state) } == 0;
        lines.push(format!(
            "XInput | index {index} | {}",
            if connected { "connected" } else { "-" }
        ));
    }
}
