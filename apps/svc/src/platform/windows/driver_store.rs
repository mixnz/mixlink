//! Driver packages in the driver store (spec 0014): add without UI, learn the published name,
//! remove by published name.

use std::path::{Path, PathBuf};

use windows::Win32::Devices::DeviceAndDriverInstallation::{
    DIINSTALLDRIVER_FLAGS, DIUNINSTALLDRIVER_FLAGS, DiInstallDriverW, DiUninstallDriverW,
    SP_COPY_NOOVERWRITE, SPOST_NONE, SetupCopyOEMInfW, SetupSetNonInteractiveMode,
};
use windows::Win32::Foundation::ERROR_FILE_EXISTS;
use windows::Win32::System::SystemInformation::GetSystemWindowsDirectoryW;
use windows::core::{HSTRING, PCWSTR};

use crate::error::SvcError;

/// SetupAPI fails instead of showing any UI for the rest of this process (spec 0003 D).
pub fn set_non_interactive() {
    // SAFETY: no pointers; returns the previous mode, which we do not need.
    let _ = unsafe { SetupSetNonInteractiveMode(true) };
}

/// Adds the package to the driver store (never forced) and returns its published name.
pub fn install_package(inf: &Path) -> Result<String, SvcError> {
    let inf_w = HSTRING::from(inf.as_os_str());
    // SAFETY: the path outlives the call; no window; flags 0 = no DIIRFLAG_FORCE_INF.
    unsafe { DiInstallDriverW(None, &inf_w, DIINSTALLDRIVER_FLAGS(0), None) }
        .map_err(SvcError::win32("DiInstallDriverW"))?;
    published_name(&inf_w)
}

/// `oemNN.inf` of a package already in the store: with SP_COPY_NOOVERWRITE, SetupCopyOEMInfW fails
/// with ERROR_FILE_EXISTS and names the existing copy.
fn published_name(inf: &HSTRING) -> Result<String, SvcError> {
    let mut dest = [0u16; 260];
    // SAFETY: the source path outlives the call; `dest` is a writable buffer.
    let copied = unsafe {
        SetupCopyOEMInfW(
            inf,
            PCWSTR::null(),
            SPOST_NONE,
            SP_COPY_NOOVERWRITE,
            Some(&mut dest),
            None,
            None,
        )
    };
    match copied {
        Ok(()) => {}
        Err(error) if error.code() == ERROR_FILE_EXISTS.to_hresult() => {}
        Err(error) => return Err(SvcError::win32("SetupCopyOEMInfW")(error)),
    }
    let end = dest.iter().position(|&c| c == 0).unwrap_or(dest.len());
    let full = PathBuf::from(String::from_utf16_lossy(&dest[..end]));
    full.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .filter(|n| !n.is_empty())
        .ok_or_else(|| SvcError::Driver {
            step: "install",
            detail: "Windows did not report the published INF name".to_owned(),
        })
}

/// `%SystemRoot%\INF\<published>`.
fn published_path(published: &str) -> PathBuf {
    let mut buf = [0u16; 260];
    // SAFETY: `buf` is a writable buffer; returns the length written.
    let len = unsafe { GetSystemWindowsDirectoryW(Some(&mut buf)) } as usize;
    let windows = if len == 0 || len >= buf.len() {
        PathBuf::from(r"C:\Windows")
    } else {
        PathBuf::from(String::from_utf16_lossy(&buf[..len]))
    };
    windows.join("INF").join(published)
}

pub fn is_published(published: &str) -> bool {
    published_path(published).is_file()
}

/// Removes the package from the driver store; already gone is fine.
pub fn uninstall_package(published: &str) -> Result<(), SvcError> {
    let path = published_path(published);
    if !path.is_file() {
        return Ok(());
    }
    // SAFETY: the path outlives the call; no window.
    unsafe {
        DiUninstallDriverW(
            None,
            &HSTRING::from(path.as_os_str()),
            DIUNINSTALLDRIVER_FLAGS(0),
            None,
        )
    }
    .map_err(SvcError::win32("DiUninstallDriverW"))
}
