use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::*;
use crate::drivers::package::DRIVERS;

/// Records every call; fails the first call whose log line starts with `fail_on`.
#[derive(Default)]
struct FakeHost {
    log: Vec<String>,
    fail_on: Option<String>,
    state: DriverState,
    saved: Option<DriverState>,
    certs: Vec<String>,
    next_cert: u32,
    next_oem: u32,
    /// Driver name → published name to return instead of the next `oemNN.inf`.
    republish: BTreeMap<String, String>,
    held: Option<&'static str>,
    no_devices: bool,
}

impl FakeHost {
    fn new() -> FakeHost {
        FakeHost {
            next_cert: 1,
            next_oem: 10,
            ..FakeHost::default()
        }
    }

    fn call(&mut self, line: String) -> Result<(), SvcError> {
        let fail = self
            .fail_on
            .as_ref()
            .is_some_and(|f| line.starts_with(f.as_str()));
        self.log.push(line.clone());
        if fail {
            self.fail_on = None;
            return Err(SvcError::Driver {
                step: "fake",
                detail: line,
            });
        }
        Ok(())
    }

    fn index(&self, prefix: &str) -> Vec<usize> {
        let found: Vec<usize> = (0..self.log.len())
            .filter(|&i| self.log[i].starts_with(prefix))
            .collect();
        found
    }

    fn has(&self, prefix: &str) -> bool {
        !self.index(prefix).is_empty()
    }
}

impl DriverHost for FakeHost {
    fn stage(&mut self, spec: &DriverSpec, source: &Path) -> Result<PathBuf, SvcError> {
        self.call(format!("stage {}", spec.name))?;
        Ok(source.join("staged"))
    }
    fn create_cert(&mut self) -> Result<String, SvcError> {
        let thumb = format!("T{}", self.next_cert);
        self.next_cert += 1;
        self.call(format!("create_cert {thumb}"))?;
        Ok(thumb)
    }
    fn sign_catalog(&mut self, thumbprint: &str, cat: &Path) -> Result<(), SvcError> {
        let name = cat.file_name().map(|n| n.to_string_lossy().into_owned());
        self.call(format!("sign {thumbprint} {}", name.unwrap_or_default()))
    }
    fn delete_key(&mut self, thumbprint: &str) -> Result<(), SvcError> {
        self.call(format!("delete_key {thumbprint}"))
    }
    fn trust_cert(&mut self, thumbprint: &str) -> Result<(), SvcError> {
        self.call(format!("trust {thumbprint}"))?;
        self.certs.push(thumbprint.to_owned());
        Ok(())
    }
    fn our_certs(&mut self) -> Result<Vec<String>, SvcError> {
        self.call("our_certs".to_owned())?;
        Ok(self.certs.clone())
    }
    fn untrust_cert(&mut self, thumbprint: &str) -> Result<(), SvcError> {
        self.call(format!("untrust {thumbprint}"))?;
        self.certs.retain(|c| c != thumbprint);
        Ok(())
    }
    fn verify_package(&mut self, _staged: &Path, cat: &Path) -> Result<(), SvcError> {
        let name = cat.file_name().map(|n| n.to_string_lossy().into_owned());
        self.call(format!("verify {}", name.unwrap_or_default()))
    }
    fn install_package(&mut self, inf: &Path) -> Result<String, SvcError> {
        let file = inf.file_name().map(|n| n.to_string_lossy().into_owned());
        let file = file.unwrap_or_default();
        self.call(format!("install_package {file}"))?;
        let driver = file.trim_end_matches(".inf").to_owned();
        if let Some(name) = self.republish.get(&driver) {
            return Ok(name.clone());
        }
        let name = format!("oem{}.inf", self.next_oem);
        self.next_oem += 1;
        Ok(name)
    }
    fn ensure_device(&mut self, spec: &DriverSpec) -> Result<(), SvcError> {
        self.call(format!("ensure_device {}", spec.name))
    }
    fn remove_devices(&mut self, spec: &DriverSpec) -> Result<RemoveOutcome, SvcError> {
        self.call(format!("remove_devices {}", spec.name))?;
        if spec.name == "vhid"
            && let Some(app) = self.held
        {
            let app = if app.is_empty() {
                None
            } else {
                Some(app.to_owned())
            };
            return Ok(RemoveOutcome::Held { app });
        }
        Ok(if self.no_devices {
            RemoveOutcome::NoDevice
        } else {
            RemoveOutcome::Removed
        })
    }
    fn uninstall_package(&mut self, published: &str) -> Result<(), SvcError> {
        self.call(format!("uninstall_package {published}"))
    }
    fn load_state(&mut self) -> Result<DriverState, SvcError> {
        self.call("load_state".to_owned())?;
        Ok(self.state.clone())
    }
    fn save_state(&mut self, state: &DriverState) -> Result<(), SvcError> {
        self.call("save_state".to_owned())?;
        self.saved = Some(state.clone());
        Ok(())
    }
    fn clear_state(&mut self) -> Result<(), SvcError> {
        self.call("clear_state".to_owned())
    }
}

fn sources() -> Vec<(&'static DriverSpec, PathBuf)> {
    DRIVERS
        .iter()
        .map(|d| (d, PathBuf::from(format!("src_{}", d.name))))
        .collect()
}

fn state(thumb: &str, vdisplay: &str, vhid: &str) -> DriverState {
    let mut s = DriverState {
        cert_thumbprint: Some(thumb.to_owned()),
        ..DriverState::default()
    };
    s.published
        .insert("vdisplay".to_owned(), vdisplay.to_owned());
    s.published.insert("vhid".to_owned(), vhid.to_owned());
    s
}

/// A machine with nothing of ours on it.
fn clean_host() -> FakeHost {
    let mut host = FakeHost::new();
    host.no_devices = true;
    host
}

/// The steps of `uninstall` on a clean machine, which `install` always runs first.
const CLEAN_UNINSTALL: [&str; 5] = [
    "load_state",
    "remove_devices vdisplay",
    "remove_devices vhid",
    "our_certs",
    "clear_state",
];

#[test]
fn install_fresh_order() {
    let mut host = clean_host();
    let outcome = install(&mut host, &sources()).unwrap();
    assert_eq!(outcome, InstallOutcome::Installed);
    let mut expected: Vec<&str> = CLEAN_UNINSTALL.to_vec();
    expected.extend([
        "stage vdisplay",
        "stage vhid",
        "create_cert T1",
        "sign T1 vdisplay.cat",
        "sign T1 vhid.cat",
        "delete_key T1",
        "trust T1",
        "verify vdisplay.cat",
        "verify vhid.cat",
        "install_package vdisplay.inf",
        "install_package vhid.inf",
        "ensure_device vdisplay",
        "ensure_device vhid",
        "save_state",
    ]);
    assert_eq!(host.log, expected);
    assert_eq!(host.saved, Some(state("T1", "oem10.inf", "oem11.inf")));
}

#[test]
fn install_devices_after_all_packages() {
    let mut host = clean_host();
    install(&mut host, &sources()).unwrap();
    let last_package = *host.index("install_package").last().unwrap();
    assert!(
        host.index("ensure_device")
            .iter()
            .all(|&i| i > last_package)
    );
}

#[test]
fn install_key_deleted_before_trust() {
    let mut host = clean_host();
    install(&mut host, &sources()).unwrap();
    assert!(host.index("delete_key")[0] < host.index("trust")[0]);
}

#[test]
fn install_sign_failure_deletes_key() {
    let mut host = clean_host();
    host.fail_on = Some("sign T1 vhid.cat".to_owned());
    assert!(install(&mut host, &sources()).is_err());
    assert!(host.has("delete_key T1"));
    assert!(!host.has("trust"));
    assert!(!host.has("save_state"));
}

#[test]
fn install_verify_failure_rolls_back() {
    let mut host = clean_host();
    host.fail_on = Some("verify vhid.cat".to_owned());
    assert!(install(&mut host, &sources()).is_err());
    assert!(host.has("untrust T1"));
    assert!(!host.has("install_package"));
    assert!(!host.has("save_state"));
    assert!(host.certs.is_empty());
}

#[test]
fn install_package_failure_records_cert() {
    let mut host = clean_host();
    host.fail_on = Some("install_package vhid.inf".to_owned());
    assert!(install(&mut host, &sources()).is_err());
    let mut expected = DriverState {
        cert_thumbprint: Some("T1".to_owned()),
        ..DriverState::default()
    };
    expected
        .published
        .insert("vdisplay".to_owned(), "oem10.inf".to_owned());
    assert_eq!(host.saved, Some(expected));
    assert!(!host.has("ensure_device"));
}

/// Reinstalling the same build: Windows keeps an identical package as it is (spec 0014 → Results),
/// so everything old goes first and the install starts from a clean machine.
#[test]
fn install_again_uninstalls_everything_first() {
    let mut host = FakeHost::new();
    host.state = state("T0", "oem3.inf", "oem4.inf");
    host.certs = vec!["T0".to_owned()];
    let outcome = install(&mut host, &sources()).unwrap();
    assert_eq!(outcome, InstallOutcome::Updated);
    let first_stage = host.index("stage")[0];
    for step in [
        "remove_devices vdisplay",
        "remove_devices vhid",
        "uninstall_package oem3.inf",
        "uninstall_package oem4.inf",
        "untrust T0",
        "clear_state",
    ] {
        assert!(host.index(step)[0] < first_stage, "{step} after staging");
    }
    assert_eq!(host.certs, ["T1"]);
    assert_eq!(host.saved, Some(state("T1", "oem10.inf", "oem11.inf")));
}

#[test]
fn install_held_changes_nothing_else() {
    let mut host = FakeHost::new();
    host.state = state("T0", "oem3.inf", "oem4.inf");
    host.certs = vec!["T0".to_owned()];
    host.held = Some("Steam");
    let outcome = install(&mut host, &sources()).unwrap();
    assert_eq!(
        outcome,
        InstallOutcome::Held {
            app: Some("Steam".to_owned())
        }
    );
    assert!(!host.has("uninstall_package"));
    assert!(!host.has("untrust"));
    assert!(!host.has("stage"));
    assert!(!host.has("create_cert"));
}

#[test]
fn install_removes_stray_certs_first() {
    let mut host = clean_host();
    host.certs = vec!["TX".to_owned()];
    let outcome = install(&mut host, &sources()).unwrap();
    assert_eq!(outcome, InstallOutcome::Updated);
    assert!(host.index("untrust TX")[0] < host.index("stage")[0]);
    assert_eq!(host.certs, ["T1"]);
}

#[test]
fn install_uninstall_failure_stops() {
    let mut host = FakeHost::new();
    host.state = state("T0", "oem3.inf", "oem4.inf");
    host.fail_on = Some("uninstall_package oem4.inf".to_owned());
    assert!(install(&mut host, &sources()).is_err());
    assert!(!host.has("stage"));
    assert!(!host.has("create_cert"));
}

#[test]
fn uninstall_held_stops_before_packages() {
    let mut host = FakeHost::new();
    host.state = state("T0", "oem3.inf", "oem4.inf");
    host.certs = vec!["T0".to_owned()];
    host.held = Some("");
    let outcome = uninstall(&mut host, &DRIVERS).unwrap();
    assert_eq!(outcome, UninstallOutcome::Held { app: None });
    assert!(!host.has("uninstall_package"));
    assert!(!host.has("untrust"));
    assert!(!host.has("clear_state"));
}

#[test]
fn uninstall_held_names_app() {
    let mut host = FakeHost::new();
    host.held = Some("Steam");
    let outcome = uninstall(&mut host, &DRIVERS).unwrap();
    assert_eq!(
        outcome,
        UninstallOutcome::Held {
            app: Some("Steam".to_owned())
        }
    );
}

#[test]
fn uninstall_order() {
    let mut host = FakeHost::new();
    host.state = state("T0", "oem3.inf", "oem4.inf");
    host.certs = vec!["T0".to_owned()];
    let outcome = uninstall(&mut host, &DRIVERS).unwrap();
    assert_eq!(outcome, UninstallOutcome::Removed);
    assert_eq!(
        host.log,
        [
            "load_state",
            "remove_devices vdisplay",
            "remove_devices vhid",
            "uninstall_package oem3.inf",
            "uninstall_package oem4.inf",
            "our_certs",
            "untrust T0",
            "clear_state",
        ]
    );
}

#[test]
fn uninstall_nothing() {
    let mut host = FakeHost::new();
    host.no_devices = true;
    let outcome = uninstall(&mut host, &DRIVERS).unwrap();
    assert_eq!(outcome, UninstallOutcome::NothingInstalled);
    assert!(host.has("clear_state"));
}

#[test]
fn uninstall_removes_certs_without_state() {
    let mut host = FakeHost::new();
    host.no_devices = true;
    host.certs = vec!["TX".to_owned()];
    let outcome = uninstall(&mut host, &DRIVERS).unwrap();
    assert_eq!(outcome, UninstallOutcome::Removed);
    assert!(host.has("untrust TX"));
}

#[test]
fn uninstall_package_failure_keeps_certs() {
    let mut host = FakeHost::new();
    host.state = state("T0", "oem3.inf", "oem4.inf");
    host.certs = vec!["T0".to_owned()];
    host.fail_on = Some("uninstall_package oem4.inf".to_owned());
    assert!(uninstall(&mut host, &DRIVERS).is_err());
    assert!(!host.has("untrust"));
    assert!(!host.has("clear_state"));
}
