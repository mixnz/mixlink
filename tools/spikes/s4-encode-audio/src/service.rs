//! Minimal Windows service host: runs one work function, stops on SCM stop/shutdown or when it returns.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};

use windows::Win32::Foundation::{ERROR_FAILED_SERVICE_CONTROLLER_CONNECT, NO_ERROR};
use windows::Win32::System::Services::{
    RegisterServiceCtrlHandlerExW, SERVICE_ACCEPT_SHUTDOWN, SERVICE_ACCEPT_STOP,
    SERVICE_CONTROL_SHUTDOWN, SERVICE_CONTROL_STOP, SERVICE_RUNNING, SERVICE_STATUS,
    SERVICE_STATUS_CURRENT_STATE, SERVICE_STATUS_HANDLE, SERVICE_STOP_PENDING, SERVICE_STOPPED,
    SERVICE_TABLE_ENTRYW, SERVICE_WIN32_OWN_PROCESS, SetServiceStatus, StartServiceCtrlDispatcherW,
};
use windows::core::{PWSTR, w};

use crate::util::BoxResult;

pub const SERVICE_NAME: &str = "MixLinkS4";

type Work = Box<dyn Fn(&AtomicBool) + Send + Sync>;

static WORK: OnceLock<Work> = OnceLock::new();
static STOP: AtomicBool = AtomicBool::new(false);
static STATUS: AtomicIsize = AtomicIsize::new(0);

fn set_state(state: SERVICE_STATUS_CURRENT_STATE) {
    let status = SERVICE_STATUS {
        dwServiceType: SERVICE_WIN32_OWN_PROCESS,
        dwCurrentState: state,
        dwControlsAccepted: if state == SERVICE_RUNNING {
            SERVICE_ACCEPT_STOP | SERVICE_ACCEPT_SHUTDOWN
        } else {
            0
        },
        dwWin32ExitCode: NO_ERROR.0,
        dwWaitHint: if state == SERVICE_STOP_PENDING {
            10_000
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

unsafe extern "system" fn handler(
    control: u32,
    _event: u32,
    _data: *mut core::ffi::c_void,
    _ctx: *mut core::ffi::c_void,
) -> u32 {
    if control == SERVICE_CONTROL_STOP || control == SERVICE_CONTROL_SHUTDOWN {
        STOP.store(true, Ordering::Release);
        set_state(SERVICE_STOP_PENDING);
    }
    NO_ERROR.0
}

unsafe extern "system" fn service_main(_argc: u32, _argv: *mut PWSTR) {
    // SAFETY: static name and handler; no context pointer.
    let Ok(handle) =
        (unsafe { RegisterServiceCtrlHandlerExW(w!("MixLinkS4"), Some(handler), None) })
    else {
        return;
    };
    STATUS.store(handle.0 as isize, Ordering::Release);
    set_state(SERVICE_RUNNING);
    if let Some(work) = WORK.get() {
        work(&STOP);
    }
    set_state(SERVICE_STOPPED);
}

/// Blocks until the service stops. Fails with a clear message when not started by the SCM.
pub fn run(work: Work) -> BoxResult<()> {
    let _ = WORK.set(work);
    let table = [
        SERVICE_TABLE_ENTRYW {
            lpServiceName: PWSTR(w!("MixLinkS4").as_ptr().cast_mut()),
            lpServiceProc: Some(service_main),
        },
        SERVICE_TABLE_ENTRYW::default(),
    ];
    // SAFETY: the table is null-terminated and outlives the dispatcher call.
    match unsafe { StartServiceCtrlDispatcherW(table.as_ptr()) } {
        Ok(()) => Ok(()),
        Err(e) if e.code() == ERROR_FAILED_SERVICE_CONTROLLER_CONNECT.to_hresult() => {
            Err(format!("not started by the service manager (install with install-svc.ps1, service {SERVICE_NAME})").into())
        }
        Err(e) => Err(e.into()),
    }
}
