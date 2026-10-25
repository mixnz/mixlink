//! Our certificate in `LocalMachine\Root` and `LocalMachine\TrustedPublisher` (ADR-0004, spec 0014):
//! add the public part, find every copy by subject (also leftovers of a broken run), remove by thumbprint.

use windows::Win32::Security::Cryptography::{
    CERT_CONTEXT, CERT_FIND_SHA1_HASH, CERT_FIND_SUBJECT_STR_W, CERT_OPEN_STORE_FLAGS,
    CERT_QUERY_ENCODING_TYPE, CERT_STORE_ADD_REPLACE_EXISTING, CERT_STORE_OPEN_EXISTING_FLAG,
    CERT_STORE_PROV_SYSTEM_W, CERT_STORE_READONLY_FLAG, CERT_SYSTEM_STORE_LOCAL_MACHINE,
    CertAddEncodedCertificateToStore, CertCloseStore, CertDeleteCertificateFromStore,
    CertFindCertificateInStore, CertFreeCertificateContext, CertOpenStore, HCERTSTORE,
    X509_ASN_ENCODING,
};
use windows::core::HSTRING;

use super::cert::{SUBJECT_NAME, SigningCert, subject_of, thumbprint_of};
use super::cert_der::blob;
use crate::error::SvcError;

const TRUST_STORES: [&str; 2] = ["Root", "TrustedPublisher"];

fn from_hex(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok())
        .collect()
}

struct Store(HCERTSTORE);

impl Drop for Store {
    fn drop(&mut self) {
        // SAFETY: the handle came from CertOpenStore and is closed once.
        let _ = unsafe { CertCloseStore(Some(self.0), 0) };
    }
}

fn open_machine_store(name: &str, read_only: bool) -> Result<Store, SvcError> {
    let mut flags = CERT_SYSTEM_STORE_LOCAL_MACHINE | CERT_STORE_OPEN_EXISTING_FLAG.0;
    if read_only {
        flags |= CERT_STORE_READONLY_FLAG.0;
    }
    let name = HSTRING::from(name);
    // SAFETY: the store name outlives the call; CERT_STORE_PROV_SYSTEM_W takes a wide string.
    let store = unsafe {
        CertOpenStore(
            CERT_STORE_PROV_SYSTEM_W,
            CERT_QUERY_ENCODING_TYPE(0),
            None,
            CERT_OPEN_STORE_FLAGS(flags),
            Some(name.as_ptr().cast()),
        )
    }
    .map_err(SvcError::win32("CertOpenStore"))?;
    Ok(Store(store))
}

/// Adds the public certificate to `LocalMachine\Root` and `LocalMachine\TrustedPublisher`.
pub fn trust(cert: &SigningCert) -> Result<(), SvcError> {
    for name in TRUST_STORES {
        let store = open_machine_store(name, false)?;
        // SAFETY: valid store; the encoded bytes outlive the call.
        unsafe {
            CertAddEncodedCertificateToStore(
                Some(store.0),
                X509_ASN_ENCODING,
                cert.encoded(),
                CERT_STORE_ADD_REPLACE_EXISTING,
                None,
            )
        }
        .map_err(SvcError::win32("CertAddEncodedCertificateToStore"))?;
    }
    Ok(())
}

/// Thumbprints of every certificate named `SUBJECT_NAME` in either trust store.
pub fn our_cert_thumbprints() -> Result<Vec<String>, SvcError> {
    let subject = HSTRING::from(SUBJECT_NAME);
    let mut found = Vec::new();
    for name in TRUST_STORES {
        let store = open_machine_store(name, true)?;
        let mut prev: *const CERT_CONTEXT = std::ptr::null();
        loop {
            // SAFETY: valid store; `prev` is null or the context the last call returned (freed by
            // this call); the subject string outlives the call.
            let ctx = unsafe {
                CertFindCertificateInStore(
                    store.0,
                    X509_ASN_ENCODING,
                    0,
                    CERT_FIND_SUBJECT_STR_W,
                    Some(subject.as_ptr().cast()),
                    (!prev.is_null()).then_some(prev),
                )
            };
            if ctx.is_null() {
                break;
            }
            prev = ctx;
            if subject_of(ctx).is_ok_and(|s| s == SUBJECT_NAME) {
                match thumbprint_of(ctx) {
                    Ok(thumb) if !found.contains(&thumb) => found.push(thumb),
                    Ok(_) => {}
                    Err(error) => {
                        // SAFETY: `ctx` came from CertFindCertificateInStore and is freed once.
                        let _ = unsafe { CertFreeCertificateContext(Some(ctx)) };
                        return Err(error);
                    }
                }
            }
        }
    }
    Ok(found)
}

/// Removes the certificate from both trust stores; missing is fine.
pub fn untrust(thumbprint: &str) -> Result<(), SvcError> {
    let mut hash =
        from_hex(thumbprint).ok_or_else(|| SvcError::StateInvalid(thumbprint.to_owned()))?;
    let hash_blob = blob(&mut hash)?;
    for name in TRUST_STORES {
        let store = open_machine_store(name, false)?;
        loop {
            // SAFETY: valid store; `hash_blob` points into `hash`, which outlives the call.
            let ctx = unsafe {
                CertFindCertificateInStore(
                    store.0,
                    X509_ASN_ENCODING,
                    0,
                    CERT_FIND_SHA1_HASH,
                    Some((&raw const hash_blob).cast()),
                    None,
                )
            };
            if ctx.is_null() {
                break;
            }
            // SAFETY: `ctx` came from CertFindCertificateInStore; the call frees it.
            unsafe { CertDeleteCertificateFromStore(ctx) }
                .map_err(SvcError::win32("CertDeleteCertificateFromStore"))?;
        }
    }
    Ok(())
}
