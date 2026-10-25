//! DER encoding helpers for the signing certificate (spec 0014): X.500 names and extensions.

use std::ffi::c_void;

use windows::Win32::Security::Cryptography::{
    CERT_X500_NAME_STR, CRYPT_ENCODE_OBJECT_FLAGS, CRYPT_INTEGER_BLOB, CertStrToNameW,
    CryptEncodeObjectEx, X509_ASN_ENCODING,
};
use windows::core::{HSTRING, PCSTR};

use crate::error::SvcError;

pub fn blob(bytes: &mut [u8]) -> Result<CRYPT_INTEGER_BLOB, SvcError> {
    Ok(CRYPT_INTEGER_BLOB {
        cbData: u32::try_from(bytes.len()).map_err(|_| too_big("blob"))?,
        pbData: bytes.as_mut_ptr(),
    })
}

fn too_big(what: &'static str) -> SvcError {
    SvcError::Driver {
        step: "certificate",
        detail: format!("{what} too large"),
    }
}

pub fn encode_name(x500: &str) -> Result<Vec<u8>, SvcError> {
    let name = HSTRING::from(x500);
    let mut len = 0u32;
    // SAFETY: size query; `len` is a valid out pointer.
    unsafe {
        CertStrToNameW(
            X509_ASN_ENCODING,
            &name,
            CERT_X500_NAME_STR,
            None,
            None,
            &mut len,
            None,
        )
    }
    .map_err(SvcError::win32("CertStrToNameW"))?;
    let mut bytes = vec![0u8; len as usize];
    // SAFETY: `bytes` holds `len` bytes as the size query asked for.
    unsafe {
        CertStrToNameW(
            X509_ASN_ENCODING,
            &name,
            CERT_X500_NAME_STR,
            None,
            Some(bytes.as_mut_ptr()),
            &mut len,
            None,
        )
    }
    .map_err(SvcError::win32("CertStrToNameW"))?;
    bytes.truncate(len as usize);
    Ok(bytes)
}

pub fn encode_object(struct_type: PCSTR, value: *const c_void) -> Result<Vec<u8>, SvcError> {
    let mut len = 0u32;
    // SAFETY: size query; `value` points to a live struct of `struct_type`.
    unsafe {
        CryptEncodeObjectEx(
            X509_ASN_ENCODING,
            struct_type,
            value,
            CRYPT_ENCODE_OBJECT_FLAGS(0),
            None,
            None,
            &mut len,
        )
    }
    .map_err(SvcError::win32("CryptEncodeObjectEx"))?;
    let mut bytes = vec![0u8; len as usize];
    // SAFETY: `bytes` holds `len` bytes as the size query asked for.
    unsafe {
        CryptEncodeObjectEx(
            X509_ASN_ENCODING,
            struct_type,
            value,
            CRYPT_ENCODE_OBJECT_FLAGS(0),
            None,
            Some(bytes.as_mut_ptr().cast()),
            &mut len,
        )
    }
    .map_err(SvcError::win32("CryptEncodeObjectEx"))?;
    bytes.truncate(len as usize);
    Ok(bytes)
}
