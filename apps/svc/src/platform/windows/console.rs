//! Ctrl+C / Ctrl+Break / console close stop `--console` mode.

use std::sync::{Arc, OnceLock};

use windows::Win32::System::Console::SetConsoleCtrlHandler;
use windows::core::BOOL;

use crate::error::SvcError;
use crate::stop::StopSignal;

static STOP: OnceLock<Arc<StopSignal>> = OnceLock::new();

unsafe extern "system" fn on_ctrl(_ctrl_type: u32) -> BOOL {
    if let Some(stop) = STOP.get() {
        stop.stop();
    }
    true.into()
}

pub fn install_ctrl_handler(stop: Arc<StopSignal>) -> Result<(), SvcError> {
    let _ = STOP.set(stop);
    // SAFETY: `on_ctrl` is a static function that only touches a static.
    unsafe { SetConsoleCtrlHandler(Some(on_ctrl), true) }
        .map_err(SvcError::win32("SetConsoleCtrlHandler"))
}
