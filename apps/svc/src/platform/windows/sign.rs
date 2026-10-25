//! Authenticode-signs a file (our driver catalogs) with `SignerSignEx3` (spec 0014): SHA-256, no
//! timestamp, the key found through the certificate's key provider info.

use std::path::Path;

use windows::Win32::Foundation::{HANDLE, HWND};
use windows::Win32::Security::Cryptography::{
    CALG_SHA_256, HCERTSTORE, SIGNER_CERT, SIGNER_CERT_0, SIGNER_CERT_POLICY_CHAIN,
    SIGNER_CERT_STORE, SIGNER_CERT_STORE_INFO, SIGNER_CONTEXT, SIGNER_FILE_INFO, SIGNER_NO_ATTR,
    SIGNER_SIGN_FLAGS, SIGNER_SIGNATURE_INFO, SIGNER_SUBJECT_FILE, SIGNER_SUBJECT_INFO,
    SIGNER_SUBJECT_INFO_0, SignerFreeSignerContext, SignerSignEx3,
};
use windows::core::{HSTRING, PCSTR, PCWSTR};

use super::cert::SigningCert;
use crate::error::SvcError;

fn size_of_u32<T>() -> u32 {
    u32::try_from(size_of::<T>()).unwrap_or(u32::MAX)
}

/// Signs `path` in place with `cert` (whose private key must still exist).
pub fn sign_file(cert: &SigningCert, path: &Path) -> Result<(), SvcError> {
    let path_w = HSTRING::from(path.as_os_str());
    let mut file_info = SIGNER_FILE_INFO {
        cbSize: size_of_u32::<SIGNER_FILE_INFO>(),
        pwszFileName: PCWSTR(path_w.as_ptr()),
        hFile: HANDLE::default(),
    };
    let mut index = 0u32;
    let subject = SIGNER_SUBJECT_INFO {
        cbSize: size_of_u32::<SIGNER_SUBJECT_INFO>(),
        pdwIndex: &mut index,
        dwSubjectChoice: SIGNER_SUBJECT_FILE,
        Anonymous: SIGNER_SUBJECT_INFO_0 {
            pSignerFileInfo: &mut file_info,
        },
    };
    let mut store_info = SIGNER_CERT_STORE_INFO {
        cbSize: size_of_u32::<SIGNER_CERT_STORE_INFO>(),
        pSigningCert: cert.context(),
        dwCertPolicy: SIGNER_CERT_POLICY_CHAIN,
        hCertStore: HCERTSTORE::default(),
    };
    let signer_cert = SIGNER_CERT {
        cbSize: size_of_u32::<SIGNER_CERT>(),
        dwCertChoice: SIGNER_CERT_STORE,
        Anonymous: SIGNER_CERT_0 {
            pCertStoreInfo: &mut store_info,
        },
        hwnd: HWND::default(),
    };
    let signature = SIGNER_SIGNATURE_INFO {
        cbSize: size_of_u32::<SIGNER_SIGNATURE_INFO>(),
        algidHash: CALG_SHA_256,
        dwAttrChoice: SIGNER_NO_ATTR,
        ..SIGNER_SIGNATURE_INFO::default()
    };
    let mut context: *mut SIGNER_CONTEXT = std::ptr::null_mut();
    // SAFETY: every struct is a live local with its cbSize set; the certificate context is valid
    // while `cert` lives; `context` receives a signer context freed below.
    let signed = unsafe {
        SignerSignEx3(
            SIGNER_SIGN_FLAGS(0),
            &subject,
            &signer_cert,
            &signature,
            None,
            None,
            PCSTR::null(),
            PCWSTR::null(),
            None,
            None,
            &mut context,
            None,
            None,
            None,
        )
    };
    if !context.is_null() {
        // SAFETY: the context came from SignerSignEx3 and is freed once.
        let _ = unsafe { SignerFreeSignerContext(context) };
    }
    signed.map_err(SvcError::win32("SignerSignEx3"))
}
