//! Service-manager glue: dispatcher, control handler, status reports.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::{Arc, OnceLock};

use windows::Win32::Foundation::{
    ERROR_FAILED_SERVICE_CONTROLLER_CONNECT, ERROR_SERVICE_SPECIFIC_ERROR, NO_ERROR,
};
use windows::Win32::System::Services::{
    RegisterServiceCtrlHandlerExW, SERVICE_ACCEPT_SHUTDOWN, SERVICE_ACCEPT_STOP,
    SERVICE_CONTROL_SHUTDOWN, SERVICE_CONTROL_STOP, SERVICE_RUNNING, SERVICE_START_PENDING,
    SERVICE_STATUS, SERVICE_STATUS_CURRENT_STATE, SERVICE_STATUS_HANDLE, SERVICE_STOP_PENDING,
    SERVICE_STOPPED, SERVICE_TABLE_ENTRYW, SERVICE_WIN32_OWN_PROCESS, SetServiceStatus,
    StartServiceCtrlDispatcherW,
};
use windows::core::{PWSTR, w};

use mixlink_core::svc_ipc::PIPE_NAME;

use crate::error::SvcError;
use crate::log_setup::init_file_log;
use crate::run::{RunOptions, run};
use crate::stop::StopSignal;
use crate::strings;

pub const SERVICE_NAME: &str = "MixLink";

const STOP_WAIT_HINT_MS: u32 = 5000;
/// Service-specific exit code reported when `run` fails or panics (recovery then restarts us).
const EXIT_FAILED: u32 = 1;

static STOP: OnceLock<Arc<StopSignal>> = OnceLock::new();
static STATUS: AtomicIsize = AtomicIsize::new(0);

/// Hands the process to the service manager; returns when the service has stopped.
pub fn dispatch() -> Result<(), SvcError> {
    let table = [
        SERVICE_TABLE_ENTRYW {
            lpServiceName: PWSTR(w!("MixLink").as_ptr().cast_mut()),
            lpServiceProc: Some(service_main),
        },
        SERVICE_TABLE_ENTRYW::default(),
    ];
    // SAFETY: the table is null-terminated and outlives the dispatcher call.
    match unsafe { StartServiceCtrlDispatcherW(table.as_ptr()) } {
        Ok(()) => Ok(()),
        Err(error) if error.code() == ERROR_FAILED_SERVICE_CONTROLLER_CONNECT.to_hresult() => {
            Err(SvcError::Usage(strings::NOT_BY_SCM.to_owned()))
        }
        Err(error) => Err(SvcError::win32("StartServiceCtrlDispatcherW")(error)),
    }
}

unsafe extern "system" fn service_main(_argc: u32, _argv: *mut PWSTR) {
    let stop = Arc::clone(STOP.get_or_init(|| Arc::new(StopSignal::new())));
    // SAFETY: static name and handler; no context pointer.
    let Ok(handle) = (unsafe { RegisterServiceCtrlHandlerExW(w!("MixLink"), Some(handler), None) })
    else {
        return;
    };
    STATUS.store(handle.0 as isize, Ordering::Release);
    set_state(SERVICE_START_PENDING, 0);
    if init_file_log().is_err() {
        // No log to write to; the exit code is all the service manager gets.
        set_state(SERVICE_STOPPED, EXIT_FAILED);
        return;
    }
    let opts = RunOptions {
        pipe: PIPE_NAME.to_owned(),
        display: Some(crate::host_display_settings()),
    };
    let result = catch_unwind(AssertUnwindSafe(|| {
        run(&stop, &opts, || set_state(SERVICE_RUNNING, 0))
    }));
    let exit = match result {
        Ok(Ok(())) => 0,
        Ok(Err(error)) => {
            log::error!("service failed: {error}");
            EXIT_FAILED
        }
        Err(_) => {
            log::error!("service panicked");
            EXIT_FAILED
        }
    };
    set_state(SERVICE_STOPPED, exit);
}

unsafe extern "system" fn handler(
    control: u32,
    _event: u32,
    _data: *mut core::ffi::c_void,
    _ctx: *mut core::ffi::c_void,
) -> u32 {
    if control == SERVICE_CONTROL_STOP || control == SERVICE_CONTROL_SHUTDOWN {
        set_state(SERVICE_STOP_PENDING, 0);
        if let Some(stop) = STOP.get() {
            stop.stop();
        }
    }
    NO_ERROR.0
}

/// Reports `state`; a non-zero `exit` is sent as a service-specific error code.
fn set_state(state: SERVICE_STATUS_CURRENT_STATE, exit: u32) {
    let status = SERVICE_STATUS {
        dwServiceType: SERVICE_WIN32_OWN_PROCESS,
        dwCurrentState: state,
        dwControlsAccepted: if state == SERVICE_RUNNING {
            SERVICE_ACCEPT_STOP | SERVICE_ACCEPT_SHUTDOWN
        } else {
            0
        },
        dwWin32ExitCode: if exit == 0 {
            NO_ERROR.0
        } else {
            ERROR_SERVICE_SPECIFIC_ERROR.0
        },
        dwServiceSpecificExitCode: exit,
        dwWaitHint: if state == SERVICE_STOP_PENDING {
            STOP_WAIT_HINT_MS
        } else {
            0
        },
        ..Default::default()
    };
    let handle = SERVICE_STATUS_HANDLE(STATUS.load(Ordering::Acquire) as *mut core::ffi::c_void);
    // SAFETY: handle comes from RegisterServiceCtrlHandlerExW; `status` lives for the call.
    unsafe {
        let _ = SetServiceStatus(handle, &status);
    }
}
