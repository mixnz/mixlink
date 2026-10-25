//! Register / remove the `MixLink` service with the service manager (needs admin).

use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{
    ERROR_ACCESS_DENIED, ERROR_SERVICE_DOES_NOT_EXIST, ERROR_SERVICE_EXISTS,
    ERROR_SERVICE_NOT_ACTIVE,
};
use windows::Win32::System::Services::{
    ChangeServiceConfig2W, ChangeServiceConfigW, CloseServiceHandle, ControlService,
    CreateServiceW, DeleteService, OpenSCManagerW, OpenServiceW, QueryServiceStatus, SC_ACTION,
    SC_ACTION_RESTART, SC_HANDLE, SC_MANAGER_ALL_ACCESS, SC_MANAGER_CONNECT, SERVICE_ALL_ACCESS,
    SERVICE_AUTO_START, SERVICE_CONFIG_DESCRIPTION, SERVICE_CONFIG_FAILURE_ACTIONS,
    SERVICE_CONFIG_FAILURE_ACTIONS_FLAG, SERVICE_CONTROL_STOP, SERVICE_DESCRIPTIONW,
    SERVICE_ERROR_NORMAL, SERVICE_FAILURE_ACTIONS_FLAG, SERVICE_FAILURE_ACTIONSW,
    SERVICE_QUERY_STATUS, SERVICE_STATUS, SERVICE_STOPPED, SERVICE_WIN32_OWN_PROCESS,
    StartServiceW,
};
use windows::core::{HSTRING, PCWSTR, PWSTR};

use super::service::SERVICE_NAME;
use crate::error::SvcError;
use crate::strings;

const RESTART_DELAY_MS: u32 = 5000;
const RESTART_ATTEMPTS: usize = 3;
const FAILURE_RESET_S: u32 = 86_400;
const STOP_TIMEOUT: Duration = Duration::from_secs(10);
const STOP_POLL: Duration = Duration::from_millis(200);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallOutcome {
    Created,
    Updated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UninstallOutcome {
    Removed,
    NotInstalled,
}

/// Closes a service-manager handle on drop.
struct ScHandle(SC_HANDLE);

impl Drop for ScHandle {
    fn drop(&mut self) {
        // SAFETY: the handle came from OpenSCManagerW / OpenServiceW / CreateServiceW and is closed once.
        let _ = unsafe { CloseServiceHandle(self.0) };
    }
}

/// The service image path, quoted so a path with spaces cannot be misread.
pub fn image_path(exe: &Path) -> String {
    format!("\"{}\"", exe.display())
}

/// Creates the service, or updates it to this exe and these settings; then (re)starts it.
pub fn install() -> Result<InstallOutcome, SvcError> {
    let exe = std::env::current_exe()?;
    let path = HSTRING::from(image_path(&exe));
    let name = HSTRING::from(SERVICE_NAME);
    let display = HSTRING::from(strings::SERVICE_DISPLAY_NAME);
    let manager = open_manager()?;
    // SAFETY: all strings outlive the call; None = LocalSystem, no password.
    let created = unsafe {
        CreateServiceW(
            manager.0,
            &name,
            &display,
            SERVICE_ALL_ACCESS,
            SERVICE_WIN32_OWN_PROCESS,
            SERVICE_AUTO_START,
            SERVICE_ERROR_NORMAL,
            &path,
            PCWSTR::null(),
            None,
            PCWSTR::null(),
            PCWSTR::null(),
            PCWSTR::null(),
        )
    };
    let (service, outcome) = match created {
        Ok(handle) => (ScHandle(handle), InstallOutcome::Created),
        Err(error) if error.code() == ERROR_SERVICE_EXISTS.to_hresult() => {
            let service = open_service(&manager)?;
            // SAFETY: as above; null strings leave those fields unchanged.
            unsafe {
                ChangeServiceConfigW(
                    service.0,
                    SERVICE_WIN32_OWN_PROCESS,
                    SERVICE_AUTO_START,
                    SERVICE_ERROR_NORMAL,
                    &path,
                    PCWSTR::null(),
                    None,
                    PCWSTR::null(),
                    PCWSTR::null(),
                    PCWSTR::null(),
                    &display,
                )
            }
            .map_err(SvcError::win32("ChangeServiceConfigW"))?;
            stop_and_wait(&service)?;
            (service, InstallOutcome::Updated)
        }
        Err(error) => return Err(access_or(error, "CreateServiceW")),
    };
    configure(&service)?;
    // SAFETY: valid service handle; no start arguments.
    unsafe { StartServiceW(service.0, None) }.map_err(SvcError::win32("StartServiceW"))?;
    Ok(outcome)
}

/// Stops and deletes the service.
pub fn uninstall() -> Result<UninstallOutcome, SvcError> {
    let manager = open_manager()?;
    let name = HSTRING::from(SERVICE_NAME);
    // SAFETY: valid manager handle; `name` outlives the call.
    let service = match unsafe { OpenServiceW(manager.0, &name, SERVICE_ALL_ACCESS) } {
        Ok(handle) => ScHandle(handle),
        Err(error) if error.code() == ERROR_SERVICE_DOES_NOT_EXIST.to_hresult() => {
            return Ok(UninstallOutcome::NotInstalled);
        }
        Err(error) => return Err(access_or(error, "OpenServiceW")),
    };
    stop_and_wait(&service)?;
    // SAFETY: valid service handle opened with DELETE access.
    unsafe { DeleteService(service.0) }.map_err(SvcError::win32("DeleteService"))?;
    Ok(UninstallOutcome::Removed)
}

fn open_manager() -> Result<ScHandle, SvcError> {
    // SAFETY: local machine, default database.
    unsafe { OpenSCManagerW(PCWSTR::null(), PCWSTR::null(), SC_MANAGER_ALL_ACCESS) }
        .map(ScHandle)
        .map_err(|error| access_or(error, "OpenSCManagerW"))
}

fn open_service(manager: &ScHandle) -> Result<ScHandle, SvcError> {
    let name = HSTRING::from(SERVICE_NAME);
    // SAFETY: valid manager handle; `name` outlives the call.
    unsafe { OpenServiceW(manager.0, &name, SERVICE_ALL_ACCESS) }
        .map(ScHandle)
        .map_err(|error| access_or(error, "OpenServiceW"))
}

fn access_or(error: windows::core::Error, call: &'static str) -> SvcError {
    if error.code() == ERROR_ACCESS_DENIED.to_hresult() {
        SvcError::NotAdmin
    } else {
        SvcError::Win32 {
            call,
            source: error,
        }
    }
}

/// Description and recovery: restart after 5 s on the first three failures, reset after a day.
fn configure(service: &ScHandle) -> Result<(), SvcError> {
    let mut description: Vec<u16> = strings::SERVICE_DESCRIPTION
        .encode_utf16()
        .chain([0])
        .collect();
    let info = SERVICE_DESCRIPTIONW {
        lpDescription: PWSTR(description.as_mut_ptr()),
    };
    // SAFETY: `info` and `description` outlive the call.
    unsafe {
        ChangeServiceConfig2W(
            service.0,
            SERVICE_CONFIG_DESCRIPTION,
            Some((&raw const info).cast()),
        )
    }
    .map_err(SvcError::win32("ChangeServiceConfig2W(description)"))?;

    let mut actions = [SC_ACTION {
        Type: SC_ACTION_RESTART,
        Delay: RESTART_DELAY_MS,
    }; RESTART_ATTEMPTS];
    let failure = SERVICE_FAILURE_ACTIONSW {
        dwResetPeriod: FAILURE_RESET_S,
        lpRebootMsg: PWSTR::null(),
        lpCommand: PWSTR::null(),
        cActions: u32::try_from(actions.len()).unwrap_or(0),
        lpsaActions: actions.as_mut_ptr(),
    };
    // SAFETY: `failure` and `actions` outlive the call.
    unsafe {
        ChangeServiceConfig2W(
            service.0,
            SERVICE_CONFIG_FAILURE_ACTIONS,
            Some((&raw const failure).cast()),
        )
    }
    .map_err(SvcError::win32("ChangeServiceConfig2W(failure actions)"))?;

    // Also restart when the service stops itself with an error, not only on a crash.
    let flag = SERVICE_FAILURE_ACTIONS_FLAG {
        fFailureActionsOnNonCrashFailures: true.into(),
    };
    // SAFETY: `flag` outlives the call.
    unsafe {
        ChangeServiceConfig2W(
            service.0,
            SERVICE_CONFIG_FAILURE_ACTIONS_FLAG,
            Some((&raw const flag).cast()),
        )
    }
    .map_err(SvcError::win32("ChangeServiceConfig2W(failure flag)"))
}

/// Whether the `MixLink` service is running (any state but stopped); not installed → `false`.
pub fn is_running() -> Result<bool, SvcError> {
    // SAFETY: local machine, default database, connect right only.
    let manager = unsafe { OpenSCManagerW(PCWSTR::null(), PCWSTR::null(), SC_MANAGER_CONNECT) }
        .map(ScHandle)
        .map_err(SvcError::win32("OpenSCManagerW"))?;
    let name = HSTRING::from(SERVICE_NAME);
    // SAFETY: valid manager handle; `name` outlives the call.
    let service = match unsafe { OpenServiceW(manager.0, &name, SERVICE_QUERY_STATUS) } {
        Ok(handle) => ScHandle(handle),
        Err(error) if error.code() == ERROR_SERVICE_DOES_NOT_EXIST.to_hresult() => {
            return Ok(false);
        }
        Err(error) => return Err(SvcError::win32("OpenServiceW")(error)),
    };
    let mut status = SERVICE_STATUS::default();
    // SAFETY: valid service handle; `status` is written by the call.
    unsafe { QueryServiceStatus(service.0, &mut status) }
        .map_err(SvcError::win32("QueryServiceStatus"))?;
    Ok(status.dwCurrentState != SERVICE_STOPPED)
}

/// Sends Stop (if running) and waits up to 10 s for the stopped state.
fn stop_and_wait(service: &ScHandle) -> Result<(), SvcError> {
    let mut status = SERVICE_STATUS::default();
    // SAFETY: valid service handle; `status` is written by the call.
    match unsafe { ControlService(service.0, SERVICE_CONTROL_STOP, &mut status) } {
        Ok(()) => {}
        Err(error) if error.code() == ERROR_SERVICE_NOT_ACTIVE.to_hresult() => return Ok(()),
        Err(error) => return Err(SvcError::win32("ControlService")(error)),
    }
    let deadline = Instant::now() + STOP_TIMEOUT;
    loop {
        // SAFETY: as above.
        unsafe { QueryServiceStatus(service.0, &mut status) }
            .map_err(SvcError::win32("QueryServiceStatus"))?;
        if status.dwCurrentState == SERVICE_STOPPED {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(SvcError::StopTimeout);
        }
        thread::sleep(STOP_POLL);
    }
}

#[cfg(test)]
#[path = "install_tests.rs"]
mod tests;
