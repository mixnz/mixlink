//! Driver install state in the registry (spec 0014 → State): one key, `REG_SZ` values only.
//! The root is a parameter so tests can use `HKEY_CURRENT_USER`; the service uses `STATE_KEY` under HKLM.

use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_NO_MORE_ITEMS, WIN32_ERROR};
use windows::Win32::System::Registry::{
    HKEY, KEY_READ, KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SZ, RegCloseKey, RegCreateKeyExW,
    RegDeleteKeyW, RegDeleteTreeW, RegEnumValueW, RegOpenKeyExW, RegSetValueExW,
};
use windows::core::{HSTRING, PCWSTR, PWSTR};

use crate::drivers::state::DriverState;
use crate::error::SvcError;

/// Under `HKEY_LOCAL_MACHINE`.
pub const STATE_KEY: &str = r"SOFTWARE\MixLink\Drivers";

/// Longest value name / data we accept (our names and data are far shorter).
const MAX_CHARS: usize = 256;

fn check(code: WIN32_ERROR, call: &'static str) -> Result<(), SvcError> {
    if code.is_ok() {
        Ok(())
    } else {
        Err(SvcError::win32(call)(code.to_hresult().into()))
    }
}

struct Key(HKEY);

impl Drop for Key {
    fn drop(&mut self) {
        // SAFETY: the handle came from RegOpenKeyExW / RegCreateKeyExW and is closed once.
        let _ = unsafe { RegCloseKey(self.0) };
    }
}

/// The saved state; a missing key is the empty state.
pub fn load_state(root: HKEY, path: &str) -> Result<DriverState, SvcError> {
    let mut handle = HKEY::default();
    // SAFETY: valid root key and out pointer; the path outlives the call.
    let opened = unsafe { RegOpenKeyExW(root, &HSTRING::from(path), None, KEY_READ, &mut handle) };
    if opened == ERROR_FILE_NOT_FOUND {
        return Ok(DriverState::default());
    }
    check(opened, "RegOpenKeyExW")?;
    let key = Key(handle);
    let mut values = Vec::new();
    for index in 0u32.. {
        let mut name = [0u16; MAX_CHARS];
        let mut name_len = MAX_CHARS as u32;
        let mut data = [0u16; MAX_CHARS];
        let mut data_len = u32::try_from(size_of_val(&data)).unwrap_or(0);
        let mut kind = 0u32;
        // SAFETY: open key; every buffer is a live local and its length is passed with it.
        let read = unsafe {
            RegEnumValueW(
                key.0,
                index,
                Some(PWSTR(name.as_mut_ptr())),
                &mut name_len,
                None,
                Some(&mut kind),
                Some(data.as_mut_ptr().cast()),
                Some(&mut data_len),
            )
        };
        if read == ERROR_NO_MORE_ITEMS {
            break;
        }
        check(read, "RegEnumValueW")?;
        let name = String::from_utf16_lossy(&name[..(name_len as usize).min(MAX_CHARS)]);
        if kind != REG_SZ.0 {
            return Err(SvcError::StateInvalid(format!("{name}: not a string")));
        }
        let chars = (data_len as usize / 2).min(MAX_CHARS);
        let text = String::from_utf16_lossy(&data[..chars]);
        values.push((name, text.trim_end_matches('\0').to_owned()));
    }
    DriverState::from_values(values)
}

/// Replaces every saved value with `state`.
pub fn save_state(root: HKEY, path: &str, state: &DriverState) -> Result<(), SvcError> {
    delete_state(root, path)?;
    write_values(root, path, &state.to_values())
}

/// Creates the key when missing and writes `REG_SZ` values.
pub fn write_values(root: HKEY, path: &str, values: &[(String, String)]) -> Result<(), SvcError> {
    let mut handle = HKEY::default();
    // SAFETY: valid root key and out pointer; the path outlives the call; default security.
    let created = unsafe {
        RegCreateKeyExW(
            root,
            &HSTRING::from(path),
            None,
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_WRITE,
            None,
            &mut handle,
            None,
        )
    };
    check(created, "RegCreateKeyExW")?;
    let key = Key(handle);
    for (name, data) in values {
        let bytes: Vec<u8> = data
            .encode_utf16()
            .chain([0])
            .flat_map(u16::to_le_bytes)
            .collect();
        // SAFETY: open key; the name and data outlive the call.
        let set = unsafe {
            RegSetValueExW(
                key.0,
                &HSTRING::from(name.as_str()),
                None,
                REG_SZ,
                Some(&bytes),
            )
        };
        check(set, "RegSetValueExW")?;
    }
    Ok(())
}

/// Deletes the key and everything in it; missing is fine.
pub fn delete_state(root: HKEY, path: &str) -> Result<(), SvcError> {
    let path = HSTRING::from(path);
    // SAFETY: valid root key; the path outlives both calls.
    let tree = unsafe { RegDeleteTreeW(root, &path) };
    if tree == ERROR_FILE_NOT_FOUND {
        return Ok(());
    }
    check(tree, "RegDeleteTreeW")?;
    // SAFETY: as above; RegDeleteTreeW leaves the (now empty) key itself.
    let key = unsafe { RegDeleteKeyW(root, &path) };
    if key == ERROR_FILE_NOT_FOUND {
        return Ok(());
    }
    check(key, "RegDeleteKeyW")
}

#[cfg(test)]
#[path = "drivers_reg_tests.rs"]
mod tests;
