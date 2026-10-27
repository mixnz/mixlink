//! Is the pipe's server the MixLink service (spec 0018)? Any program could create
//! `\.\pipe\mixlink-svc` before the service does and show a PIN of its choice in the tray.

use std::fs::File;
use std::io;
use std::mem::{offset_of, size_of};
use std::os::windows::io::AsRawHandle;

use mixlink_core::svc_ipc::SERVICE_NAME;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::Pipes::GetNamedPipeServerProcessId;
use windows::Win32::System::Services::{
    CloseServiceHandle, OpenSCManagerW, OpenServiceW, QueryServiceStatusEx, SC_HANDLE,
    SC_MANAGER_CONNECT, SC_STATUS_PROCESS_INFO, SERVICE_QUERY_STATUS, SERVICE_STATUS_PROCESS,
};
use windows::core::{HSTRING, PCWSTR};

struct ScHandle(SC_HANDLE);

impl Drop for ScHandle {
    fn drop(&mut self) {
        // SAFETY: the handle came from OpenSCManagerW / OpenServiceW and is closed once.
        let _ = unsafe { CloseServiceHandle(self.0) };
    }
}

/// True when the process serving `pipe` is the running MixLink service.
pub fn server_is_service(pipe: &File) -> io::Result<bool> {
    let mut server = 0u32;
    // SAFETY: `pipe` is an open client end of a named pipe; `server` is a live out pointer.
    unsafe { GetNamedPipeServerProcessId(HANDLE(pipe.as_raw_handle()), &mut server) }
        .map_err(io::Error::other)?;
    Ok(service_pid()? == Some(server))
}

/// The service's process id; `None` when it is not installed or not running.
fn service_pid() -> io::Result<Option<u32>> {
    // SAFETY: local machine, default database, connect right only.
    let manager = unsafe { OpenSCManagerW(PCWSTR::null(), PCWSTR::null(), SC_MANAGER_CONNECT) }
        .map(ScHandle)
        .map_err(io::Error::other)?;
    let name = HSTRING::from(SERVICE_NAME);
    // SAFETY: valid manager handle; `name` outlives the call.
    let Ok(service) = (unsafe { OpenServiceW(manager.0, &name, SERVICE_QUERY_STATUS) }) else {
        return Ok(None);
    };
    let service = ScHandle(service);
    let mut buf = [0u8; size_of::<SERVICE_STATUS_PROCESS>()];
    let mut needed = 0u32;
    // SAFETY: valid service handle; `buf` has room for SERVICE_STATUS_PROCESS, `needed` is live.
    unsafe {
        QueryServiceStatusEx(
            service.0,
            SC_STATUS_PROCESS_INFO,
            Some(&mut buf),
            &mut needed,
        )
    }
    .map_err(io::Error::other)?;
    let at = offset_of!(SERVICE_STATUS_PROCESS, dwProcessId);
    let pid = u32::from_ne_bytes([buf[at], buf[at + 1], buf[at + 2], buf[at + 3]]);
    Ok((pid != 0).then_some(pid))
}
