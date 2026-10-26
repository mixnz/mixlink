//! The `input ...` developer commands (spec 0016).

use std::io::Write;

use mixlink_input::platform::windows::vhid::VHid;

use super::status::format_status;
use crate::args::InputCmd;
use crate::error::SvcError;

pub fn run(cmd: &InputCmd) -> Result<(), SvcError> {
    match cmd {
        InputCmd::Status => status(),
    }
}

fn status() -> Result<(), SvcError> {
    let reply = VHid::open()?.query()?;
    std::io::stdout().write_all(format_status(&reply).as_bytes())?;
    Ok(())
}
