//! `DriverHost` on Windows (spec 0014): wires the flow's steps to the certificate, catalog, driver
//! store, device and registry modules. Also the elevation check and the `status` query.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Security::{GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation};
use windows::Win32::System::Registry::HKEY_LOCAL_MACHINE;
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

use super::cert::{KeyScope, SigningCert, create_signing_cert};
use super::drivers_reg::{STATE_KEY, delete_state, load_state, save_state};
use super::{catalog, devices, dir_security, driver_store, sign, trust_store};
use crate::drivers::flow::{DriverHost, RemoveOutcome};
use crate::drivers::package::{DRIVERS, DriverSpec, stage_dir};
use crate::drivers::state::DriverState;
use crate::drivers::status::{DriverStatus, StatusReport};
use crate::error::SvcError;

/// `%ProgramData%` (as the service log uses it).
pub fn program_data() -> PathBuf {
    PathBuf::from(std::env::var_os("ProgramData").unwrap_or_else(|| r"C:\ProgramData".into()))
}

/// True when this process runs with a full (elevated) administrator token.
pub fn is_elevated() -> Result<bool, SvcError> {
    let mut token = HANDLE::default();
    // SAFETY: pseudo handle of this process; `token` is a valid out pointer, closed below.
    unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) }
        .map_err(SvcError::win32("OpenProcessToken"))?;
    let mut elevation = TOKEN_ELEVATION::default();
    let mut len = 0u32;
    // SAFETY: valid token; `elevation` is sized for TokenElevation.
    let got = unsafe {
        GetTokenInformation(
            token,
            TokenElevation,
            Some((&raw mut elevation).cast()),
            u32::try_from(size_of::<TOKEN_ELEVATION>()).unwrap_or(0),
            &mut len,
        )
    };
    // SAFETY: the token handle came from OpenProcessToken and is closed once.
    let _ = unsafe { CloseHandle(token) };
    got.map_err(SvcError::win32("GetTokenInformation"))?;
    Ok(elevation.TokenIsElevated != 0)
}

#[derive(Debug)]
pub struct WindowsDriverHost {
    program_data: PathBuf,
    cert: Option<SigningCert>,
    /// Driver name → staged INF, for `ensure_device`.
    staged_inf: BTreeMap<&'static str, PathBuf>,
}

impl WindowsDriverHost {
    /// Fails with `NotAdmin` unless elevated; then turns SetupAPI UI off for this process.
    pub fn new() -> Result<WindowsDriverHost, SvcError> {
        if !is_elevated()? {
            return Err(SvcError::NotAdmin);
        }
        driver_store::set_non_interactive();
        Ok(WindowsDriverHost {
            program_data: program_data(),
            cert: None,
            staged_inf: BTreeMap::new(),
        })
    }

    fn cert(&mut self, thumbprint: &str) -> Result<&mut SigningCert, SvcError> {
        match &mut self.cert {
            Some(cert) if cert.thumbprint() == thumbprint => Ok(cert),
            _ => Err(SvcError::Driver {
                step: "certificate",
                detail: format!("no signing certificate {thumbprint} in this run"),
            }),
        }
    }

    fn stage_root(&self) -> PathBuf {
        self.program_data.join("MixLink").join("drivers")
    }
}

impl DriverHost for WindowsDriverHost {
    fn stage(&mut self, spec: &DriverSpec, source: &Path) -> Result<PathBuf, SvcError> {
        dir_security::secure_dir(&self.program_data.join("MixLink"))?;
        dir_security::secure_dir(&self.stage_root())?;
        let dir = stage_dir(&self.program_data, spec.name);
        if dir.exists() {
            fs::remove_dir_all(&dir)?;
        }
        fs::create_dir(&dir)?;
        for entry in fs::read_dir(source)? {
            let path = entry?.path();
            if let Some(name) = path.file_name().filter(|_| path.is_file()) {
                fs::copy(&path, dir.join(name))?;
            }
        }
        self.staged_inf.insert(spec.name, dir.join(spec.inf));
        Ok(dir)
    }

    fn create_cert(&mut self) -> Result<String, SvcError> {
        let cert = create_signing_cert(KeyScope::Machine)?;
        let thumbprint = cert.thumbprint();
        self.cert = Some(cert);
        Ok(thumbprint)
    }

    fn sign_catalog(&mut self, thumbprint: &str, cat: &Path) -> Result<(), SvcError> {
        let cert = self.cert(thumbprint)?;
        sign::sign_file(cert, cat)
    }

    fn delete_key(&mut self, thumbprint: &str) -> Result<(), SvcError> {
        self.cert(thumbprint)?.delete_key()
    }

    fn trust_cert(&mut self, thumbprint: &str) -> Result<(), SvcError> {
        let cert = self.cert(thumbprint)?;
        trust_store::trust(cert)
    }

    fn our_certs(&mut self) -> Result<Vec<String>, SvcError> {
        trust_store::our_cert_thumbprints()
    }

    fn untrust_cert(&mut self, thumbprint: &str) -> Result<(), SvcError> {
        trust_store::untrust(thumbprint)
    }

    fn verify_package(&mut self, staged: &Path, cat: &Path) -> Result<(), SvcError> {
        catalog::check_members(staged, cat)?;
        catalog::verify_signature(cat)
    }

    fn install_package(&mut self, inf: &Path) -> Result<String, SvcError> {
        driver_store::install_package(inf)
    }

    fn ensure_device(&mut self, spec: &DriverSpec) -> Result<(), SvcError> {
        let inf = self.staged_inf.get(spec.name).ok_or(SvcError::Driver {
            step: "device",
            detail: format!("{} was not staged", spec.name),
        })?;
        devices::ensure_device(spec, inf)
    }

    fn remove_devices(&mut self, spec: &DriverSpec) -> Result<RemoveOutcome, SvcError> {
        devices::remove_devices(spec)
    }

    fn uninstall_package(&mut self, published: &str) -> Result<(), SvcError> {
        driver_store::uninstall_package(published)
    }

    fn load_state(&mut self) -> Result<DriverState, SvcError> {
        load_state(HKEY_LOCAL_MACHINE, STATE_KEY)
    }

    fn save_state(&mut self, state: &DriverState) -> Result<(), SvcError> {
        save_state(HKEY_LOCAL_MACHINE, STATE_KEY, state)
    }

    fn clear_state(&mut self) -> Result<(), SvcError> {
        delete_state(HKEY_LOCAL_MACHINE, STATE_KEY)?;
        match fs::remove_dir_all(self.stage_root()) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error.into()),
            _ => Ok(()),
        }
    }
}

/// What is installed; needs no elevation (reads HKLM, the driver store and the trust stores).
pub fn query_status() -> Result<StatusReport, SvcError> {
    let state = load_state(HKEY_LOCAL_MACHINE, STATE_KEY)?;
    let mut drivers = Vec::with_capacity(DRIVERS.len());
    for spec in &DRIVERS {
        let published = state.published.get(spec.name).cloned();
        let in_store = published.as_deref().is_some_and(driver_store::is_published);
        drivers.push(DriverStatus {
            name: spec.name,
            published,
            in_store,
            device: devices::device_state(spec)?,
        });
    }
    let trusted = trust_store::our_cert_thumbprints()?;
    let cert_trusted = match &state.cert_thumbprint {
        Some(thumb) => trusted.contains(thumb),
        None => !trusted.is_empty(),
    };
    Ok(StatusReport {
        drivers,
        cert: state.cert_thumbprint,
        cert_trusted,
    })
}
