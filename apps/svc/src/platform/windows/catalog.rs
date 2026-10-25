//! Checks a driver package against its catalog (spec 0014 → Install steps 6): every file must be a
//! catalog member with an unchanged hash, and the catalog's signature must chain to a trusted root.
//! Checked here, before install, because Windows offers "Install anyway" on a changed file (spec 0003 D).

use std::fs::{self, File};
use std::os::windows::io::AsRawHandle;
use std::path::Path;

use windows::Win32::Foundation::{HANDLE, HWND, INVALID_HANDLE_VALUE};
use windows::Win32::Security::Cryptography::BCRYPT_SHA256_ALGORITHM;
use windows::Win32::Security::Cryptography::Catalog::{
    CRYPTCAT_OPEN_EXISTING, CRYPTCAT_VERSION, CryptCATAdminAcquireContext2,
    CryptCATAdminCalcHashFromFileHandle2, CryptCATAdminReleaseContext, CryptCATClose,
    CryptCATGetMemberInfo, CryptCATOpen,
};
use windows::Win32::Security::WinTrust::{
    WINTRUST_ACTION_GENERIC_VERIFY_V2, WINTRUST_DATA, WINTRUST_DATA_0, WINTRUST_FILE_INFO,
    WTD_CHOICE_FILE, WTD_REVOKE_NONE, WTD_STATEACTION_CLOSE, WTD_STATEACTION_VERIFY, WTD_UI_NONE,
    WinVerifyTrust,
};
use windows::core::{HRESULT, HSTRING, PCWSTR};

use crate::error::SvcError;

fn verify_error(detail: String) -> SvcError {
    SvcError::Driver {
        step: "verify",
        detail,
    }
}

struct CatAdmin(isize);

impl Drop for CatAdmin {
    fn drop(&mut self) {
        // SAFETY: the handle came from CryptCATAdminAcquireContext2 and is released once.
        let _ = unsafe { CryptCATAdminReleaseContext(self.0, 0) };
    }
}

struct Catalog(HANDLE);

impl Drop for Catalog {
    fn drop(&mut self) {
        // SAFETY: the handle came from CryptCATOpen and is closed once.
        let _ = unsafe { CryptCATClose(self.0) };
    }
}

/// Hash algorithms a catalog member tag may use: SHA-256 first, then SHA-1 (older catalog format).
const HASH_ALGORITHMS: [PCWSTR; 2] = [BCRYPT_SHA256_ALGORITHM, PCWSTR::null()];

/// Every file in `dir` except `cat` itself must be a member of `cat` with a matching hash.
pub fn check_members(dir: &Path, cat: &Path) -> Result<(), SvcError> {
    let cat_w = HSTRING::from(cat.as_os_str());
    // SAFETY: the path outlives the call; open-existing only, no provider.
    let handle = unsafe { CryptCATOpen(&cat_w, CRYPTCAT_OPEN_EXISTING, 0, CRYPTCAT_VERSION(0), 0) };
    if handle.is_invalid() || handle == INVALID_HANDLE_VALUE {
        return Err(verify_error(format!(
            "cannot open catalog {}",
            cat.display()
        )));
    }
    let catalog = Catalog(handle);
    let mut admins = Vec::with_capacity(HASH_ALGORITHMS.len());
    for algorithm in HASH_ALGORITHMS {
        let mut admin = 0isize;
        // SAFETY: `admin` is a valid out pointer; a null algorithm selects the SHA-1 default.
        unsafe { CryptCATAdminAcquireContext2(&mut admin, None, algorithm, None, None) }
            .map_err(SvcError::win32("CryptCATAdminAcquireContext2"))?;
        admins.push(CatAdmin(admin));
    }

    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if !path.is_file() || path == cat {
            continue;
        }
        let file = File::open(&path)?;
        let mut member = false;
        for admin in &admins {
            let tag = hash_tag(admin, &file)?;
            // SAFETY: valid catalog handle; the tag string outlives the call. The returned member
            // belongs to the catalog and is not freed here.
            if !unsafe { CryptCATGetMemberInfo(catalog.0, &HSTRING::from(tag.as_str())) }.is_null()
            {
                member = true;
                break;
            }
        }
        if !member {
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
            return Err(verify_error(format!(
                "{} is not in the catalog or was changed",
                name.unwrap_or_default()
            )));
        }
    }
    Ok(())
}

/// The file's catalog hash as upper-case hex (its member tag).
fn hash_tag(admin: &CatAdmin, file: &File) -> Result<String, SvcError> {
    let handle = HANDLE(file.as_raw_handle());
    let mut hash = [0u8; 64];
    let mut len = u32::try_from(hash.len()).unwrap_or(0);
    // SAFETY: valid admin context and open file handle; `hash` holds `len` bytes.
    unsafe {
        CryptCATAdminCalcHashFromFileHandle2(
            admin.0,
            handle,
            &mut len,
            Some(hash.as_mut_ptr()),
            None,
        )
    }
    .map_err(SvcError::win32("CryptCATAdminCalcHashFromFileHandle2"))?;
    let len = (len as usize).min(hash.len());
    Ok(hash[..len].iter().map(|b| format!("{b:02X}")).collect())
}

/// The catalog's Authenticode signature is intact and chains to a trusted root. No UI, no
/// revocation check (our certificate has no revocation point).
pub fn verify_signature(cat: &Path) -> Result<(), SvcError> {
    let cat_w = HSTRING::from(cat.as_os_str());
    let mut file_info = WINTRUST_FILE_INFO {
        cbStruct: u32::try_from(size_of::<WINTRUST_FILE_INFO>()).unwrap_or(u32::MAX),
        pcwszFilePath: PCWSTR(cat_w.as_ptr()),
        ..WINTRUST_FILE_INFO::default()
    };
    let mut data = WINTRUST_DATA {
        cbStruct: u32::try_from(size_of::<WINTRUST_DATA>()).unwrap_or(u32::MAX),
        dwUIChoice: WTD_UI_NONE,
        fdwRevocationChecks: WTD_REVOKE_NONE,
        dwUnionChoice: WTD_CHOICE_FILE,
        Anonymous: WINTRUST_DATA_0 {
            pFile: &mut file_info,
        },
        dwStateAction: WTD_STATEACTION_VERIFY,
        ..WINTRUST_DATA::default()
    };
    let mut action = WINTRUST_ACTION_GENERIC_VERIFY_V2;
    // SAFETY: `data` and `file_info` are live locals with cbStruct set; no window (UI none).
    let status = unsafe { WinVerifyTrust(HWND::default(), &mut action, (&raw mut data).cast()) };
    data.dwStateAction = WTD_STATEACTION_CLOSE;
    // SAFETY: same structures; closes the state data the verify call opened.
    let _ = unsafe { WinVerifyTrust(HWND::default(), &mut action, (&raw mut data).cast()) };
    if status != 0 {
        return Err(verify_error(format!(
            "catalog signature of {} not trusted ({})",
            cat.display(),
            HRESULT(status)
        )));
    }
    Ok(())
}

#[cfg(test)]
#[path = "catalog_tests.rs"]
mod tests;
