//! Runs a program as SYSTEM in the active console session (spec 0015): our own token, moved to
//! that session, on `winsta0\default`. Used for the display helper; the service sits in Session 0.

use std::time::Duration;

use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0};
use windows::Win32::Security::{
    DuplicateTokenEx, SecurityImpersonation, SetTokenInformation, TOKEN_ACCESS_MASK,
    TOKEN_ADJUST_DEFAULT, TOKEN_ADJUST_SESSIONID, TOKEN_ASSIGN_PRIMARY, TOKEN_DUPLICATE,
    TOKEN_QUERY, TokenPrimary, TokenSessionId,
};
use windows::Win32::System::RemoteDesktop::{ProcessIdToSessionId, WTSGetActiveConsoleSessionId};
use windows::Win32::System::Threading::{
    CREATE_NO_WINDOW, CreateProcessAsUserW, GetCurrentProcess, GetCurrentProcessId,
    GetExitCodeProcess, OpenProcessToken, PROCESS_INFORMATION, STARTUPINFOW, TerminateProcess,
    WaitForSingleObject,
};
use windows::core::{PCWSTR, PWSTR};

use crate::error::SvcError;

/// No session is attached to the physical console (between a session switch).
const NO_CONSOLE_SESSION: u32 = 0xFFFF_FFFF;
/// Exit code given to a helper killed after its timeout.
const KILLED: u32 = 0xFFFF_FFFF;

/// Closes a handle on drop.
struct Owned(HANDLE);

impl Drop for Owned {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            // SAFETY: the handle came from a Win32 call that returned it to us; closed once.
            let _ = unsafe { CloseHandle(self.0) };
        }
    }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain([0]).collect()
}

/// Runs `command_line` (program path first, quoted) in the console session and waits up to
/// `timeout`. Returns its exit code; a program still running then is killed.
pub fn run_in_console_session(command_line: &str, timeout: Duration) -> Result<u32, SvcError> {
    // SAFETY: plain query, no arguments.
    let session = unsafe { WTSGetActiveConsoleSessionId() };
    if session == NO_CONSOLE_SESSION {
        return Err(SvcError::DisplayAgent("no console session".to_owned()));
    }
    let token = primary_token_for(session)?;
    let mut desktop = wide(r"winsta0\default");
    let startup = STARTUPINFOW {
        cb: u32::try_from(std::mem::size_of::<STARTUPINFOW>()).unwrap_or(0),
        lpDesktop: PWSTR(desktop.as_mut_ptr()),
        ..Default::default()
    };
    let mut command = wide(command_line);
    let mut info = PROCESS_INFORMATION::default();
    // SAFETY: `token` is a primary token; `command`, `desktop` and `startup` outlive the call;
    // `info` receives the handles, closed below.
    unsafe {
        CreateProcessAsUserW(
            Some(token.0),
            PCWSTR::null(),
            Some(PWSTR(command.as_mut_ptr())),
            None,
            None,
            false,
            CREATE_NO_WINDOW,
            None,
            PCWSTR::null(),
            &raw const startup,
            &raw mut info,
        )
    }
    .map_err(SvcError::win32("CreateProcessAsUserW"))?;
    let process = Owned(info.hProcess);
    let _thread = Owned(info.hThread);
    let millis = u32::try_from(timeout.as_millis()).unwrap_or(u32::MAX);
    // SAFETY: valid process handle.
    if unsafe { WaitForSingleObject(process.0, millis) } != WAIT_OBJECT_0 {
        // SAFETY: valid process handle with terminate access (we created it).
        let _ = unsafe { TerminateProcess(process.0, KILLED) };
        return Err(SvcError::DisplayAgent(
            "the display helper did not answer in time".to_owned(),
        ));
    }
    let mut code = 0u32;
    // SAFETY: valid process handle; `code` is a live out pointer.
    unsafe { GetExitCodeProcess(process.0, &raw mut code) }
        .map_err(SvcError::win32("GetExitCodeProcess"))?;
    Ok(code)
}

/// Our own (SYSTEM) token as a primary token in `session`. Moving a token to another session
/// needs SeTcbPrivilege, which LocalSystem holds.
fn primary_token_for(session: u32) -> Result<Owned, SvcError> {
    let access: TOKEN_ACCESS_MASK = TOKEN_DUPLICATE
        | TOKEN_QUERY
        | TOKEN_ASSIGN_PRIMARY
        | TOKEN_ADJUST_SESSIONID
        | TOKEN_ADJUST_DEFAULT;
    let mut own = HANDLE::default();
    // SAFETY: pseudo handle of this process; `own` is a live out pointer, closed by `Owned`.
    unsafe { OpenProcessToken(GetCurrentProcess(), access, &raw mut own) }
        .map_err(SvcError::win32("OpenProcessToken"))?;
    let own = Owned(own);
    let mut primary = HANDLE::default();
    // SAFETY: valid token opened with TOKEN_DUPLICATE; `primary` is a live out pointer.
    unsafe {
        DuplicateTokenEx(
            own.0,
            access,
            None,
            SecurityImpersonation,
            TokenPrimary,
            &raw mut primary,
        )
    }
    .map_err(SvcError::win32("DuplicateTokenEx"))?;
    let primary = Owned(primary);
    // SAFETY: valid token with TOKEN_ADJUST_SESSIONID; the value is a live u32 of the given size.
    unsafe {
        SetTokenInformation(
            primary.0,
            TokenSessionId,
            (&raw const session).cast(),
            u32::try_from(std::mem::size_of::<u32>()).unwrap_or(4),
        )
    }
    .map_err(SvcError::win32("SetTokenInformation(TokenSessionId)"))?;
    Ok(primary)
}

/// The session this process runs in (0 for a service).
pub fn current_session() -> Result<u32, SvcError> {
    let mut session = 0u32;
    // SAFETY: our own process id; `session` is a live out pointer.
    unsafe { ProcessIdToSessionId(GetCurrentProcessId(), &raw mut session) }
        .map_err(SvcError::win32("ProcessIdToSessionId"))?;
    Ok(session)
}
