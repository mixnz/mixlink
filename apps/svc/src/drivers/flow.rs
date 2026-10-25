//! Install / uninstall step order and rollback (spec 0014 → Install steps, Uninstall steps), over a
//! `DriverHost` that does the real work. Kept free of Windows calls so the order is unit-tested.

use std::path::{Path, PathBuf};

use crate::drivers::package::DriverSpec;
use crate::drivers::state::DriverState;
use crate::error::SvcError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoveOutcome {
    Removed,
    NoDevice,
    /// A program holds the device open; `app` when Windows names it.
    Held {
        app: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallOutcome {
    Installed,
    /// Something of ours was there and was removed first.
    Updated,
    /// The first step (removing what was there) stopped at a device a program holds open.
    Held {
        app: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UninstallOutcome {
    Removed,
    NothingInstalled,
    Held { app: Option<String> },
}

/// The machine-changing steps. Certificates are named by their SHA-1 thumbprint.
pub trait DriverHost {
    /// Copies the package to its staging folder and returns that folder.
    fn stage(&mut self, spec: &DriverSpec, source: &Path) -> Result<PathBuf, SvcError>;
    /// Creates a persisted key and a certificate for it; returns the thumbprint.
    fn create_cert(&mut self) -> Result<String, SvcError>;
    fn sign_catalog(&mut self, thumbprint: &str, cat: &Path) -> Result<(), SvcError>;
    /// Deletes the private key and checks that it is gone.
    fn delete_key(&mut self, thumbprint: &str) -> Result<(), SvcError>;
    fn trust_cert(&mut self, thumbprint: &str) -> Result<(), SvcError>;
    /// Thumbprints of every certificate with our subject in either trust store.
    fn our_certs(&mut self) -> Result<Vec<String>, SvcError>;
    /// Removes the certificate from both trust stores; missing is fine.
    fn untrust_cert(&mut self, thumbprint: &str) -> Result<(), SvcError>;
    fn verify_package(&mut self, staged: &Path, cat: &Path) -> Result<(), SvcError>;
    /// Adds the package to the driver store; returns its published name (`oemNN.inf`).
    fn install_package(&mut self, inf: &Path) -> Result<String, SvcError>;
    /// Creates the root device node when missing, then installs the driver on it.
    fn ensure_device(&mut self, spec: &DriverSpec) -> Result<(), SvcError>;
    fn remove_devices(&mut self, spec: &DriverSpec) -> Result<RemoveOutcome, SvcError>;
    fn uninstall_package(&mut self, published: &str) -> Result<(), SvcError>;
    fn load_state(&mut self) -> Result<DriverState, SvcError>;
    fn save_state(&mut self, state: &DriverState) -> Result<(), SvcError>;
    /// Deletes the saved state and the staging folders.
    fn clear_state(&mut self) -> Result<(), SvcError>;
}

/// Installs or updates every package in `sources` (driver, unsigned package folder).
pub fn install<H: DriverHost>(
    host: &mut H,
    sources: &[(&DriverSpec, PathBuf)],
) -> Result<InstallOutcome, SvcError> {
    // Windows keeps an identical package as it is, still signed by the old certificate, so a
    // reinstall cannot swap certificates in place: remove everything, then install as on a clean
    // machine (spec 0014 → Install steps).
    let specs: Vec<DriverSpec> = sources.iter().map(|(spec, _)| **spec).collect();
    let updated = match uninstall(host, &specs)? {
        UninstallOutcome::Removed => true,
        UninstallOutcome::NothingInstalled => false,
        UninstallOutcome::Held { app } => return Ok(InstallOutcome::Held { app }),
    };

    let mut staged = Vec::with_capacity(sources.len());
    for (spec, source) in sources {
        staged.push((*spec, host.stage(spec, source)?));
    }

    let thumb = host.create_cert()?;
    let signed = staged
        .iter()
        .try_for_each(|(spec, dir)| host.sign_catalog(&thumb, &dir.join(spec.cat)));
    if let Err(error) = signed {
        // The first error is the one to report; the key goes either way.
        let _ = host.delete_key(&thumb);
        return Err(error);
    }
    host.delete_key(&thumb)?;
    host.trust_cert(&thumb)?;

    let verified = staged
        .iter()
        .try_for_each(|(spec, dir)| host.verify_package(dir, &dir.join(spec.cat)));
    if let Err(error) = verified {
        let _ = host.untrust_cert(&thumb);
        return Err(error);
    }

    // From here on the state names everything, so a later install or uninstall can clean up.
    let mut new = DriverState {
        cert_thumbprint: Some(thumb),
        ..DriverState::default()
    };
    if let Err(error) = install_packages(host, &staged, &mut new) {
        let _ = host.save_state(&new);
        return Err(error);
    }
    host.save_state(&new)?;
    Ok(if updated {
        InstallOutcome::Updated
    } else {
        InstallOutcome::Installed
    })
}

/// All packages first, then the device nodes (spec 0003 C: no node before its package is accepted).
fn install_packages<H: DriverHost>(
    host: &mut H,
    staged: &[(&DriverSpec, PathBuf)],
    state: &mut DriverState,
) -> Result<(), SvcError> {
    for (spec, dir) in staged {
        let published = host.install_package(&dir.join(spec.inf))?;
        state.published.insert(spec.name.to_owned(), published);
    }
    for (spec, _) in staged {
        host.ensure_device(spec)?;
    }
    Ok(())
}

/// Removes devices, then packages, then certificates (last, so a driver that stays is still trusted).
pub fn uninstall<H: DriverHost>(
    host: &mut H,
    specs: &[DriverSpec],
) -> Result<UninstallOutcome, SvcError> {
    let state = host.load_state()?;
    let mut removed_device = false;
    for spec in specs {
        match host.remove_devices(spec)? {
            RemoveOutcome::Removed => removed_device = true,
            RemoveOutcome::NoDevice => {}
            RemoveOutcome::Held { app } => return Ok(UninstallOutcome::Held { app }),
        }
    }
    for published in state.published.values() {
        host.uninstall_package(published)?;
    }
    let certs = host.our_certs()?;
    for cert in &certs {
        host.untrust_cert(cert)?;
    }
    host.clear_state()?;
    Ok(if state.is_empty() && !removed_device && certs.is_empty() {
        UninstallOutcome::NothingInstalled
    } else {
        UninstallOutcome::Removed
    })
}

#[cfg(test)]
#[path = "flow_tests.rs"]
mod tests;
