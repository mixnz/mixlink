//! Starts a process as SYSTEM in the active console session (ADR-0005 fallback layout).

use std::mem::size_of;
use std::sync::atomic::{AtomicBool, Ordering};

use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0};
use windows::Win32::Security::{
    DuplicateTokenEx, SecurityImpersonation, SetTokenInformation, TOKEN_ALL_ACCESS, TokenPrimary,
    TokenSessionId,
};
use windows::Win32::System::RemoteDesktop::WTSGetActiveConsoleSessionId;
use windows::Win32::System::Threading::{
    CREATE_NO_WINDOW, CreateProcessAsUserW, GetCurrentProcess, GetExitCodeProcess,
    OpenProcessToken, PROCESS_INFORMATION, STARTUPINFOW, TerminateProcess, WaitForSingleObject,
};
use windows::core::{HSTRING, PWSTR};

use crate::util::BoxResult;

/// Runs `exe args` in the console session as SYSTEM and waits; kills it if `stop` is set.
/// Returns the exit code and the session id it ran in.
pub fn run_in_console(exe: &str, args: &str, stop: &AtomicBool) -> BoxResult<(u32, u32)> {
    let mut cmdline: Vec<u16> = format!("\"{exe}\" {args}")
        .encode_utf16()
        .chain([0])
        .collect();
    let mut desktop: Vec<u16> = "winsta0\\default".encode_utf16().chain([0]).collect();
    // SAFETY: plain Win32 calls on handles created and closed here; buffers outlive the calls.
    unsafe {
        let session = WTSGetActiveConsoleSessionId();
        if session == u32::MAX {
            return Err("no active console session".into());
        }
        let mut own = HANDLE::default();
        OpenProcessToken(GetCurrentProcess(), TOKEN_ALL_ACCESS, &mut own)?;
        let mut token = HANDLE::default();
        let dup = DuplicateTokenEx(
            own,
            TOKEN_ALL_ACCESS,
            None,
            SecurityImpersonation,
            TokenPrimary,
            &mut token,
        );
        let _ = CloseHandle(own);
        dup?;
        let set = SetTokenInformation(
            token,
            TokenSessionId,
            (&session as *const u32).cast(),
            size_of::<u32>() as u32,
        );
        if let Err(e) = set {
            let _ = CloseHandle(token);
            return Err(e.into());
        }
        let si = STARTUPINFOW {
            cb: size_of::<STARTUPINFOW>() as u32,
            lpDesktop: PWSTR(desktop.as_mut_ptr()),
            ..Default::default()
        };
        let mut pi = PROCESS_INFORMATION::default();
        let created = CreateProcessAsUserW(
            Some(token),
            &HSTRING::from(exe),
            Some(PWSTR(cmdline.as_mut_ptr())),
            None,
            None,
            false,
            CREATE_NO_WINDOW,
            None,
            None,
            &si,
            &mut pi,
        );
        let _ = CloseHandle(token);
        created?;
        let _ = CloseHandle(pi.hThread);
        while WaitForSingleObject(pi.hProcess, 500) != WAIT_OBJECT_0 {
            if stop.load(Ordering::Relaxed) {
                let _ = TerminateProcess(pi.hProcess, 1);
            }
        }
        let mut code = 0u32;
        let _ = GetExitCodeProcess(pi.hProcess, &mut code);
        let _ = CloseHandle(pi.hProcess);
        Ok((code, session))
    }
}
