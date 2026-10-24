//! Named pipe server instances.

use std::os::windows::io::{FromRawHandle, OwnedHandle};

use windows::Win32::Foundation::{
    ERROR_ACCESS_DENIED, ERROR_PIPE_BUSY, ERROR_PIPE_CONNECTED, HANDLE,
};
use windows::Win32::Storage::FileSystem::{FILE_FLAG_FIRST_PIPE_INSTANCE, PIPE_ACCESS_DUPLEX};
use windows::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS,
    PIPE_TYPE_BYTE, PIPE_WAIT,
};
use windows::core::{Error, HSTRING};

use super::security::PipeSecurity;
use crate::error::SvcError;

const MAX_INSTANCES: u32 = 4;
const BUFFER_BYTES: u32 = 4096;

/// Creates one pipe instance. `Ok(None)`: all instances are in use.
/// With `first`, fails with `PipeTaken` if any process already has an instance of this name.
pub fn create_instance(
    name: &str,
    first: bool,
    security: &PipeSecurity,
) -> Result<Option<OwnedHandle>, SvcError> {
    let mut open_mode = PIPE_ACCESS_DUPLEX;
    if first {
        open_mode |= FILE_FLAG_FIRST_PIPE_INSTANCE;
    }
    let attributes = security.attributes();
    // SAFETY: `attributes` and the descriptor it points to outlive the call.
    let handle: HANDLE = unsafe {
        CreateNamedPipeW(
            &HSTRING::from(name),
            open_mode,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            MAX_INSTANCES,
            BUFFER_BYTES,
            BUFFER_BYTES,
            0,
            Some(&attributes),
        )
    };
    if handle.is_invalid() {
        let error = Error::from_thread();
        return if first && error.code() == ERROR_ACCESS_DENIED.to_hresult() {
            Err(SvcError::PipeTaken)
        } else if error.code() == ERROR_PIPE_BUSY.to_hresult() {
            Ok(None)
        } else {
            Err(SvcError::win32("CreateNamedPipeW")(error))
        };
    }
    // SAFETY: a valid handle we own exclusively from here on.
    Ok(Some(unsafe { OwnedHandle::from_raw_handle(handle.0) }))
}

/// Blocks until a client connects to `instance`.
pub fn wait_client(instance: &OwnedHandle) -> Result<(), SvcError> {
    use std::os::windows::io::AsRawHandle;
    // SAFETY: `instance` is a live pipe handle for the duration of the call; no OVERLAPPED.
    match unsafe { ConnectNamedPipe(HANDLE(instance.as_raw_handle()), None) } {
        Ok(()) => Ok(()),
        // The client connected between CreateNamedPipeW and ConnectNamedPipe.
        Err(error) if error.code() == ERROR_PIPE_CONNECTED.to_hresult() => Ok(()),
        Err(error) => Err(SvcError::win32("ConnectNamedPipe")(error)),
    }
}
