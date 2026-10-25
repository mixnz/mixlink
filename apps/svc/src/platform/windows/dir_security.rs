//! Folder ACL for staged driver packages (spec 0014): owner Administrators, protected DACL with
//! SYSTEM + Administrators only, so a standard user cannot swap files between verify and install.

use std::path::Path;

use windows::Win32::Security::Authorization::{SE_FILE_OBJECT, SetNamedSecurityInfoW};
use windows::Win32::Security::{
    ACL, DACL_SECURITY_INFORMATION, GetSecurityDescriptorDacl, GetSecurityDescriptorOwner,
    OWNER_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION, PSID,
};
use windows::core::{BOOL, HSTRING};

use super::security::PipeSecurity;
use crate::error::SvcError;

/// Inherited by every file and subfolder (OICI); `P` blocks ACEs from the parent.
pub const STAGE_SDDL: &str = "O:BAD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)";

/// Creates `path` when missing, then sets owner and DACL from `STAGE_SDDL` (also replaces an ACL
/// a standard user may have put on a folder they created first).
pub fn secure_dir(path: &Path) -> Result<(), SvcError> {
    std::fs::create_dir_all(path)?;
    let security = PipeSecurity::from_sddl(STAGE_SDDL)?;
    let mut owner = PSID::default();
    let mut defaulted = BOOL::default();
    // SAFETY: the descriptor lives as long as `security`; out pointers are live locals.
    unsafe { GetSecurityDescriptorOwner(security.descriptor(), &mut owner, &mut defaulted) }
        .map_err(SvcError::win32("GetSecurityDescriptorOwner"))?;
    let mut present = BOOL::default();
    let mut dacl: *mut ACL = std::ptr::null_mut();
    // SAFETY: as above.
    unsafe {
        GetSecurityDescriptorDacl(
            security.descriptor(),
            &mut present,
            &mut dacl,
            &mut defaulted,
        )
    }
    .map_err(SvcError::win32("GetSecurityDescriptorDacl"))?;
    // SAFETY: owner and DACL point into `security`, alive for the call; the path outlives it.
    let set = unsafe {
        SetNamedSecurityInfoW(
            &HSTRING::from(path.as_os_str()),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION
                | DACL_SECURITY_INFORMATION
                | PROTECTED_DACL_SECURITY_INFORMATION,
            Some(owner),
            None,
            Some(dacl),
            None,
        )
    };
    if set.is_err() {
        return Err(SvcError::win32("SetNamedSecurityInfoW")(
            set.to_hresult().into(),
        ));
    }
    Ok(())
}
