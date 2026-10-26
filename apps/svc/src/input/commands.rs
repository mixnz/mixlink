//! The `input ...` developer commands (spec 0016).

use std::io::Write;
use std::sync::Arc;
use std::time::Duration;

use mixlink_input::platform::windows::vhid::VHid;

use super::open_feeder;
use super::script::{Script, play, steps};
use super::status::format_status;
use crate::args::InputCmd;
use crate::error::SvcError;
use crate::platform::windows::console::install_ctrl_handler;
use crate::stop::StopSignal;
use crate::strings;

pub fn run(cmd: &InputCmd) -> Result<(), SvcError> {
    match cmd {
        InputCmd::Status => status(),
        InputCmd::Try { script, delay_s } => try_script(*script, *delay_s),
    }
}

fn status() -> Result<(), SvcError> {
    let reply = VHid::open()?.query()?;
    std::io::stdout().write_all(format_status(&reply).as_bytes())?;
    Ok(())
}

/// Plays `script` as the driver's feeder. Fails with the "in use" message while the service holds
/// the driver. Ctrl+C stops early; closing the handle releases whatever was held.
fn try_script(script: Script, delay_s: u32) -> Result<(), SvcError> {
    let mut vhid = open_feeder()?;
    let stop = Arc::new(StopSignal::new());
    install_ctrl_handler(Arc::clone(&stop))?;
    for left in (1..=delay_s).rev() {
        eprintln!("{}", strings::input_try_countdown(left));
        if stop.wait_timeout(Duration::from_secs(1)) {
            return Ok(());
        }
    }
    let mut wait = |timeout| stop.wait_timeout(timeout);
    play(&steps(script), &mut vhid, &mut wait)?;
    if script == Script::Hold {
        eprintln!("{}", strings::INPUT_TRY_HOLD);
        stop.wait();
    } else {
        eprintln!("{}", strings::INPUT_TRY_DONE);
    }
    Ok(())
}
