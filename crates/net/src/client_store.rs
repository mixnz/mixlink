//! The client's identity and paired hosts on disk (spec 0018): `%LOCALAPPDATA%\MixLink\`
//! (`identity.json`, `known_hosts.json`). Shared by the launcher and `net-probe`. Control plane.

use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use mixlink_core::ids::DeviceId;
use serde::{Deserialize, Serialize};

use crate::error::NetError;
use crate::keys::{ClientIdentity, StaticKeyPair};
use crate::pairing::KnownHost;

const FILE_VERSION: u32 = 1;

#[derive(Debug, Serialize, Deserialize)]
struct IdentityFile {
    version: u32,
    #[serde(flatten)]
    identity: ClientIdentity,
}

#[derive(Debug, Serialize, Deserialize)]
struct KnownFile {
    version: u32,
    hosts: Vec<KnownHost>,
}

/// `%LOCALAPPDATA%\MixLink`; `None` when the variable is not set.
pub fn default_dir() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|base| PathBuf::from(base).join("MixLink"))
}

fn identity_path(dir: &Path) -> PathBuf {
    dir.join("identity.json")
}

fn known_path(dir: &Path) -> PathBuf {
    dir.join("known_hosts.json")
}

fn store_error(path: &Path, error: impl std::fmt::Display) -> NetError {
    NetError::Store(format!("{}: {error}", path.display()))
}

fn read(path: &Path) -> Option<String> {
    match std::fs::read_to_string(path) {
        Ok(text) => Some(text),
        Err(error) if error.kind() == ErrorKind::NotFound => None,
        Err(error) => {
            log::warn!("cannot read {}: {error}", path.display());
            None
        }
    }
}

/// Pretty JSON, written to a temporary file, then renamed over the old one.
fn write<T: Serialize>(path: &Path, value: &T) -> Result<(), NetError> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|error| store_error(dir, error))?;
    }
    let mut text = serde_json::to_string_pretty(value).map_err(|error| store_error(path, error))?;
    text.push('\n');
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, text).map_err(|error| store_error(&tmp, error))?;
    std::fs::rename(&tmp, path).map_err(|error| store_error(path, error))
}

/// This device's id and key pair; made and saved on first use (or when the file is not valid,
/// which means pairing again with every host).
pub fn load_or_create_identity(dir: &Path) -> Result<ClientIdentity, NetError> {
    let path = identity_path(dir);
    if let Some(text) = read(&path) {
        match serde_json::from_str::<IdentityFile>(&text) {
            Ok(file) => return Ok(file.identity),
            Err(error) => log::warn!(
                "{} is not valid ({error}); making a new client identity",
                path.display()
            ),
        }
    }
    let mut id = [0u8; 16];
    getrandom::fill(&mut id).map_err(|error| NetError::Keys(format!("{error}")))?;
    let identity = ClientIdentity {
        id: DeviceId::from_bytes(id),
        keys: StaticKeyPair::generate()?,
    };
    write(
        &path,
        &IdentityFile {
            version: FILE_VERSION,
            identity: identity.clone(),
        },
    )?;
    log::info!("new client id {}", identity.id);
    Ok(identity)
}

/// Paired hosts; empty (with a warning) when the file is missing or not valid.
pub fn load_known(dir: &Path) -> Vec<KnownHost> {
    let path = known_path(dir);
    let Some(text) = read(&path) else {
        return Vec::new();
    };
    match serde_json::from_str::<KnownFile>(&text) {
        Ok(file) => file.hosts,
        Err(error) => {
            log::warn!("{} is not valid ({error}); no paired hosts", path.display());
            Vec::new()
        }
    }
}

pub fn save_known(dir: &Path, hosts: &[KnownHost]) -> Result<(), NetError> {
    write(
        &known_path(dir),
        &KnownFile {
            version: FILE_VERSION,
            hosts: hosts.to_vec(),
        },
    )
}

/// Adds or replaces one host.
pub fn remember(dir: &Path, host: KnownHost) -> Result<(), NetError> {
    let mut hosts = load_known(dir);
    hosts.retain(|known| known.host != host.host);
    hosts.push(host);
    save_known(dir, &hosts)
}

/// Removes one host; absent is fine.
pub fn forget(dir: &Path, host: DeviceId) -> Result<(), NetError> {
    let mut hosts = load_known(dir);
    let before = hosts.len();
    hosts.retain(|known| known.host != host);
    if hosts.len() == before {
        return Ok(());
    }
    save_known(dir, &hosts)
}

#[cfg(test)]
#[path = "client_store_tests.rs"]
mod tests;
