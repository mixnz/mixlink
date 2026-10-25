//! Per-machine driver signing certificate (ADR-0004, spec 0014): a fresh CNG key and a self-signed
//! code-signing certificate; the key is deleted right after the catalogs are signed, and only the
//! public certificate goes into `LocalMachine\Root` and `LocalMachine\TrustedPublisher`.

use std::time::{SystemTime, UNIX_EPOCH};

use windows::Win32::Foundation::NTE_BAD_KEYSET;
use windows::Win32::Security::Cryptography::{
    BCRYPT_RSA_ALGORITHM, CERT_CONTEXT, CERT_CREATE_SELFSIGN_FLAGS, CERT_EXTENSION,
    CERT_EXTENSIONS, CERT_KEY_SPEC, CERT_NAME_SIMPLE_DISPLAY_TYPE, CERT_SHA1_HASH_PROP_ID,
    CRYPT_ALGORITHM_IDENTIFIER, CRYPT_KEY_FLAGS, CRYPT_KEY_PROV_INFO, CRYPT_MACHINE_KEYSET,
    CTL_USAGE, CertCreateSelfSignCertificate, CertFreeCertificateContext,
    CertGetCertificateContextProperty, CertGetNameStringW, HCRYPTPROV_OR_NCRYPT_KEY_HANDLE,
    MS_KEY_STORAGE_PROVIDER, NCRYPT_FLAGS, NCRYPT_HANDLE, NCRYPT_KEY_HANDLE,
    NCRYPT_LENGTH_PROPERTY, NCRYPT_MACHINE_KEY_FLAG, NCRYPT_PROV_HANDLE, NCRYPT_SILENT_FLAG,
    NCryptCreatePersistedKey, NCryptDeleteKey, NCryptFinalizeKey, NCryptFreeObject, NCryptOpenKey,
    NCryptOpenStorageProvider, NCryptSetProperty, X509_ENHANCED_KEY_USAGE,
    szOID_ENHANCED_KEY_USAGE, szOID_PKIX_KP_CODE_SIGNING, szOID_RSA_SHA256RSA,
};
use windows::Win32::System::SystemInformation::GetSystemTime;
use windows::core::{Error, HSTRING, PSTR, PWSTR};

use super::cert_der::{blob, encode_name, encode_object};
use crate::error::SvcError;

/// Common name of every certificate we create; also how we find leftovers.
pub const SUBJECT_NAME: &str = "MixLink Driver (this PC)";
const SUBJECT_X500: &str = "CN=\"MixLink Driver (this PC)\"";
const KEY_BITS: u32 = 3072;
const VALID_YEARS: u16 = 20;

/// Where the key lives. `User` needs no admin rights: tests only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyScope {
    Machine,
    #[cfg(test)]
    User,
}

impl KeyScope {
    fn ncrypt_flags(self) -> NCRYPT_FLAGS {
        match self {
            KeyScope::Machine => NCRYPT_MACHINE_KEY_FLAG | NCRYPT_SILENT_FLAG,
            #[cfg(test)]
            KeyScope::User => NCRYPT_SILENT_FLAG,
        }
    }

    fn prov_info_flags(self) -> CRYPT_KEY_FLAGS {
        match self {
            KeyScope::Machine => CRYPT_MACHINE_KEYSET,
            #[cfg(test)]
            KeyScope::User => CRYPT_KEY_FLAGS(0),
        }
    }
}

struct Provider(NCRYPT_PROV_HANDLE);

impl Drop for Provider {
    fn drop(&mut self) {
        // SAFETY: the handle came from NCryptOpenStorageProvider and is freed once.
        let _ = unsafe { NCryptFreeObject(NCRYPT_HANDLE(self.0.0)) };
    }
}

fn open_provider() -> Result<Provider, SvcError> {
    let mut handle = NCRYPT_PROV_HANDLE::default();
    // SAFETY: `handle` is a valid out pointer; the provider name is a static wide string.
    unsafe { NCryptOpenStorageProvider(&mut handle, MS_KEY_STORAGE_PROVIDER, 0) }
        .map_err(SvcError::win32("NCryptOpenStorageProvider"))?;
    Ok(Provider(handle))
}

struct Key(NCRYPT_KEY_HANDLE);

impl Drop for Key {
    fn drop(&mut self) {
        // SAFETY: the handle came from NCryptCreatePersistedKey / NCryptOpenKey and is freed once.
        let _ = unsafe { NCryptFreeObject(NCRYPT_HANDLE(self.0.0)) };
    }
}

/// A signing certificate whose private key still exists until `delete_key` (or drop).
#[derive(Debug)]
pub struct SigningCert {
    context: *const CERT_CONTEXT,
    container: String,
    scope: KeyScope,
    key_deleted: bool,
}

/// Creates a fresh key (RSA 3072, not exportable) and a self-signed code-signing certificate.
pub fn create_signing_cert(scope: KeyScope) -> Result<SigningCert, SvcError> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let container = format!("MixLink-Driver-{}-{nanos}", std::process::id());
    let provider = open_provider()?;
    let mut handle = NCRYPT_KEY_HANDLE::default();
    // SAFETY: valid provider and out pointer; the strings outlive the call.
    unsafe {
        NCryptCreatePersistedKey(
            provider.0,
            &mut handle,
            BCRYPT_RSA_ALGORITHM,
            &HSTRING::from(container.as_str()),
            CERT_KEY_SPEC(0),
            scope.ncrypt_flags(),
        )
    }
    .map_err(SvcError::win32("NCryptCreatePersistedKey"))?;
    let key = Key(handle);
    let created = finalize_and_certify(&key, &container, scope);
    drop(key);
    match created {
        Ok(context) => Ok(SigningCert {
            context,
            container,
            scope,
            key_deleted: false,
        }),
        Err(error) => {
            let _ = delete_container(scope, &container);
            Err(error)
        }
    }
}

fn finalize_and_certify(
    key: &Key,
    container: &str,
    scope: KeyScope,
) -> Result<*const CERT_CONTEXT, SvcError> {
    // SAFETY: valid key handle; the input slice outlives the call.
    unsafe {
        NCryptSetProperty(
            NCRYPT_HANDLE(key.0.0),
            NCRYPT_LENGTH_PROPERTY,
            &KEY_BITS.to_le_bytes(),
            NCRYPT_SILENT_FLAG,
        )
    }
    .map_err(SvcError::win32("NCryptSetProperty"))?;
    // SAFETY: valid key handle that has not been finalized yet.
    unsafe { NCryptFinalizeKey(key.0, NCRYPT_SILENT_FLAG) }
        .map_err(SvcError::win32("NCryptFinalizeKey"))?;

    let mut subject = encode_name(SUBJECT_X500)?;
    let subject_blob = blob(&mut subject)?;
    let mut code_signing = PSTR(szOID_PKIX_KP_CODE_SIGNING.0.cast_mut());
    let usage = CTL_USAGE {
        cUsageIdentifier: 1,
        rgpszUsageIdentifier: &mut code_signing,
    };
    let mut eku = encode_object(X509_ENHANCED_KEY_USAGE, (&raw const usage).cast())?;
    let mut extension = CERT_EXTENSION {
        pszObjId: PSTR(szOID_ENHANCED_KEY_USAGE.0.cast_mut()),
        fCritical: false.into(),
        Value: blob(&mut eku)?,
    };
    let extensions = CERT_EXTENSIONS {
        cExtension: 1,
        rgExtension: &mut extension,
    };
    let mut container_w: Vec<u16> = container.encode_utf16().chain([0]).collect();
    let prov_info = CRYPT_KEY_PROV_INFO {
        pwszContainerName: PWSTR(container_w.as_mut_ptr()),
        pwszProvName: PWSTR(MS_KEY_STORAGE_PROVIDER.0.cast_mut()),
        dwFlags: scope.prov_info_flags(),
        ..CRYPT_KEY_PROV_INFO::default()
    };
    let algorithm = CRYPT_ALGORITHM_IDENTIFIER {
        pszObjId: PSTR(szOID_RSA_SHA256RSA.0.cast_mut()),
        ..CRYPT_ALGORITHM_IDENTIFIER::default()
    };
    // SAFETY: no arguments; returns the current UTC time.
    let start = unsafe { GetSystemTime() };
    let mut end = start;
    end.wYear += VALID_YEARS;
    if end.wMonth == 2 && end.wDay == 29 {
        end.wDay = 28;
    }
    // SAFETY: every pointer refers to a local that outlives the call; the key is finalized.
    let context = unsafe {
        CertCreateSelfSignCertificate(
            Some(HCRYPTPROV_OR_NCRYPT_KEY_HANDLE(key.0.0)),
            &subject_blob,
            CERT_CREATE_SELFSIGN_FLAGS(0),
            Some(&prov_info),
            Some(&algorithm),
            Some(&start),
            Some(&end),
            Some(&extensions),
        )
    };
    if context.is_null() {
        return Err(SvcError::win32("CertCreateSelfSignCertificate")(
            Error::from_thread(),
        ));
    }
    Ok(context)
}

impl SigningCert {
    /// SHA-1 thumbprint, 40 upper-case hex digits.
    pub fn thumbprint(&self) -> String {
        thumbprint_of(self.context).unwrap_or_default()
    }

    #[cfg(test)]
    pub fn subject(&self) -> Result<String, SvcError> {
        subject_of(self.context)
    }

    #[cfg(test)]
    pub fn container(&self) -> &str {
        &self.container
    }

    /// Valid while `self` lives.
    pub(crate) fn context(&self) -> *const CERT_CONTEXT {
        self.context
    }

    /// Deletes the private key and checks that the container is gone (spec 0003: check the
    /// container, not the key object). Deleting twice is fine.
    pub fn delete_key(&mut self) -> Result<(), SvcError> {
        delete_container(self.scope, &self.container)?;
        self.key_deleted = true;
        Ok(())
    }

    /// The DER-encoded public certificate (no key properties).
    pub(super) fn encoded(&self) -> &[u8] {
        // SAFETY: the context is valid while `self` lives; pbCertEncoded holds cbCertEncoded bytes.
        unsafe {
            let ctx = &*self.context;
            std::slice::from_raw_parts(ctx.pbCertEncoded, ctx.cbCertEncoded as usize)
        }
    }
}

impl Drop for SigningCert {
    fn drop(&mut self) {
        if !self.key_deleted {
            let _ = delete_container(self.scope, &self.container);
        }
        // SAFETY: the context came from CertCreateSelfSignCertificate and is freed once.
        let _ = unsafe { CertFreeCertificateContext(Some(self.context)) };
    }
}

fn open_key(scope: KeyScope, container: &str) -> Result<Option<Key>, SvcError> {
    let provider = open_provider()?;
    let mut handle = NCRYPT_KEY_HANDLE::default();
    // SAFETY: valid provider and out pointer; the name outlives the call.
    let opened = unsafe {
        NCryptOpenKey(
            provider.0,
            &mut handle,
            &HSTRING::from(container),
            CERT_KEY_SPEC(0),
            scope.ncrypt_flags(),
        )
    };
    match opened {
        Ok(()) => Ok(Some(Key(handle))),
        Err(error) if error.code() == NTE_BAD_KEYSET => Ok(None),
        Err(error) => Err(SvcError::win32("NCryptOpenKey")(error)),
    }
}

/// True when the key container still exists.
pub fn key_exists(scope: KeyScope, container: &str) -> Result<bool, SvcError> {
    Ok(open_key(scope, container)?.is_some())
}

fn delete_container(scope: KeyScope, container: &str) -> Result<(), SvcError> {
    if let Some(key) = open_key(scope, container)? {
        let handle = key.0;
        // NCryptDeleteKey frees the handle on success; on failure `Key` still owns it.
        std::mem::forget(key);
        // SAFETY: a valid key handle; ownership passes to the call.
        if let Err(error) = unsafe { NCryptDeleteKey(handle, NCRYPT_SILENT_FLAG.0) } {
            drop(Key(handle));
            return Err(SvcError::win32("NCryptDeleteKey")(error));
        }
    }
    if key_exists(scope, container)? {
        return Err(SvcError::KeyNotDeleted);
    }
    Ok(())
}

pub(super) fn thumbprint_of(context: *const CERT_CONTEXT) -> Result<String, SvcError> {
    let mut hash = [0u8; 20];
    let mut len = 20u32;
    // SAFETY: valid context; `hash` holds `len` bytes.
    unsafe {
        CertGetCertificateContextProperty(
            context,
            CERT_SHA1_HASH_PROP_ID,
            Some(hash.as_mut_ptr().cast()),
            &mut len,
        )
    }
    .map_err(SvcError::win32("CertGetCertificateContextProperty"))?;
    Ok(hash.iter().map(|b| format!("{b:02X}")).collect())
}

pub(super) fn subject_of(context: *const CERT_CONTEXT) -> Result<String, SvcError> {
    let mut buf = [0u16; 256];
    // SAFETY: valid context; `buf` is a writable slice the call fills with a NUL-terminated name.
    let len = unsafe {
        CertGetNameStringW(
            context,
            CERT_NAME_SIMPLE_DISPLAY_TYPE,
            0,
            None,
            Some(&mut buf),
        )
    } as usize;
    Ok(String::from_utf16_lossy(
        &buf[..len.saturating_sub(1).min(buf.len())],
    ))
}

#[cfg(test)]
#[path = "cert_tests.rs"]
mod tests;
