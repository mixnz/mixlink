//! Random ids from the system RNG.

use mixlink_core::ids::DeviceId;
use windows::Win32::Security::Cryptography::{BCRYPT_USE_SYSTEM_PREFERRED_RNG, BCryptGenRandom};

use crate::error::SvcError;

pub fn random_device_id() -> Result<DeviceId, SvcError> {
    let mut bytes = [0u8; 16];
    // SAFETY: `bytes` is a live, writable 16-byte buffer; no algorithm handle with the
    // system-preferred RNG flag.
    unsafe { BCryptGenRandom(None, &mut bytes, BCRYPT_USE_SYSTEM_PREFERRED_RNG) }
        .ok()
        .map_err(SvcError::win32("BCryptGenRandom"))?;
    Ok(DeviceId::from_bytes(bytes))
}
