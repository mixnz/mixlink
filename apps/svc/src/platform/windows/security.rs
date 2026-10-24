//! Pipe ACL (ADR-0005): SYSTEM, Administrators and the pipe's owner get full access; interactive
//! users may read and write but not create pipe instances, so they cannot pose as the server.

use std::ffi::c_void;

use windows::Win32::Foundation::{HLOCAL, LocalFree};
use windows::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows::Win32::Security::{PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};
use windows::core::HSTRING;

use crate::error::SvcError;

// 0x12008b = FILE_GENERIC_READ | FILE_WRITE_DATA (no FILE_CREATE_PIPE_INSTANCE).
// OW = owner rights: the account that created the pipe (SYSTEM as a service, the user with --console).
pub const PIPE_SDDL: &str = "D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;GA;;;OW)(A;;0x12008b;;;IU)";

/// Owned security descriptor for `CreateNamedPipeW`.
#[derive(Debug)]
pub struct PipeSecurity {
    descriptor: PSECURITY_DESCRIPTOR,
}

// SAFETY: the descriptor is immutable after creation and only read by CreateNamedPipeW.
unsafe impl Send for PipeSecurity {}
// SAFETY: as above; no interior mutability.
unsafe impl Sync for PipeSecurity {}

impl PipeSecurity {
    pub fn new() -> Result<PipeSecurity, SvcError> {
        PipeSecurity::from_sddl(PIPE_SDDL)
    }

    pub fn from_sddl(sddl: &str) -> Result<PipeSecurity, SvcError> {
        let mut descriptor = PSECURITY_DESCRIPTOR::default();
        // SAFETY: `sddl` outlives the call; `descriptor` receives a LocalAlloc'd buffer freed in Drop.
        unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                &HSTRING::from(sddl),
                SDDL_REVISION_1,
                &mut descriptor,
                None,
            )
        }
        .map_err(SvcError::win32(
            "ConvertStringSecurityDescriptorToSecurityDescriptorW",
        ))?;
        Ok(PipeSecurity { descriptor })
    }

    /// Valid while `self` lives.
    pub fn attributes(&self) -> SECURITY_ATTRIBUTES {
        SECURITY_ATTRIBUTES {
            nLength: u32::try_from(size_of::<SECURITY_ATTRIBUTES>()).unwrap_or(u32::MAX),
            lpSecurityDescriptor: self.descriptor.0.cast::<c_void>(),
            bInheritHandle: false.into(),
        }
    }
}

impl Drop for PipeSecurity {
    fn drop(&mut self) {
        // SAFETY: the buffer came from ConvertStringSecurityDescriptorToSecurityDescriptorW (LocalAlloc).
        unsafe {
            LocalFree(Some(HLOCAL(self.descriptor.0)));
        }
    }
}
