use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use super::*;
use crate::platform::windows::cert::{KeyScope, create_signing_cert};
use crate::platform::windows::sign::sign_file;

static NEXT: AtomicU32 = AtomicU32::new(0);

/// The built `vhid` package (`scripts/check-drivers.sh`); `None` (test skipped) when not built.
fn built_package() -> Option<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(r"..\..\target\drivers\vhid\pkg");
    if dir.join("vhid.cat").is_file() {
        Some(dir)
    } else {
        eprintln!(
            "skipped: no built package at {} (run scripts/check-drivers.sh)",
            dir.display()
        );
        None
    }
}

/// Copy of the package in a fresh temp folder whose name contains a space.
fn copy_package(src: &Path) -> PathBuf {
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("mixlink cat {}-{n}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    for entry in fs::read_dir(src).unwrap() {
        let path = entry.unwrap().path();
        fs::copy(&path, dir.join(path.file_name().unwrap())).unwrap();
    }
    dir
}

#[test]
fn built_package_members_ok() {
    let Some(src) = built_package() else { return };
    let dir = copy_package(&src);
    check_members(&dir, &dir.join("vhid.cat")).unwrap();
}

#[test]
fn changed_dll_fails() {
    let Some(src) = built_package() else { return };
    let dir = copy_package(&src);
    let dll = dir.join("vhid.dll");
    let mut bytes = fs::read(&dll).unwrap();
    bytes.push(0);
    fs::write(&dll, bytes).unwrap();
    let err = check_members(&dir, &dir.join("vhid.cat")).unwrap_err();
    assert!(err.to_string().contains("vhid.dll"), "{err}");
}

#[test]
fn extra_file_not_in_catalog() {
    let Some(src) = built_package() else { return };
    let dir = copy_package(&src);
    fs::write(dir.join("extra.txt"), b"x").unwrap();
    let err = check_members(&dir, &dir.join("vhid.cat")).unwrap_err();
    assert!(err.to_string().contains("extra.txt"), "{err}");
}

#[test]
fn unsigned_catalog_fails_signature() {
    let Some(src) = built_package() else { return };
    let dir = copy_package(&src);
    let err = verify_signature(&dir.join("vhid.cat"))
        .unwrap_err()
        .to_string();
    assert!(err.contains("0x800B0100"), "{err}");
}

#[test]
fn signed_by_untrusted_cert_fails_signature() {
    let Some(src) = built_package() else { return };
    let dir = copy_package(&src);
    let cat = dir.join("vhid.cat");
    let mut cert = create_signing_cert(KeyScope::User).unwrap();
    sign_file(&cert, &cat).unwrap();
    cert.delete_key().unwrap();
    let err = verify_signature(&cat).unwrap_err().to_string();
    // Signed but untrusted (CERT_E_UNTRUSTEDROOT), not unsigned (TRUST_E_NOSIGNATURE).
    assert!(err.contains("0x800B0109"), "{err}");
    check_members(&dir, &cat).unwrap();
}
