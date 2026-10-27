//! Host key and paired clients (spec 0018): `%ProgramData%\MixLink\secrets\`, which only SYSTEM and
//! Administrators can read or write (`dir_security::secure_dir`). A user who could add a key there
//! would pair themselves, so the folder ACL is set before any file is written.

use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use mixlink_net::keys::StaticKeyPair;
use mixlink_net::pairing::PairedDevice;
use serde::{Deserialize, Serialize};

use crate::error::SvcError;
use crate::json_file;

const FILE_VERSION: u32 = 1;

#[derive(Debug, Serialize, Deserialize)]
struct KeyFile {
    version: u32,
    key: StaticKeyPair,
}

#[derive(Debug, Serialize, Deserialize)]
struct PairedFile {
    version: u32,
    devices: Vec<PairedDevice>,
}

pub fn dir(program_data: &Path) -> PathBuf {
    program_data.join("MixLink").join("secrets")
}

fn key_path(dir: &Path) -> PathBuf {
    dir.join("host_key.json")
}

fn paired_path(dir: &Path) -> PathBuf {
    dir.join("paired.json")
}

/// Text of `path`, or `None` (logged) when missing or unreadable.
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

/// The saved host key; a new one from `new_key` (saved) when missing or not usable. A new key
/// means every client has to pair again.
pub fn load_or_create_key(
    dir: &Path,
    new_key: impl FnOnce() -> Result<StaticKeyPair, SvcError>,
) -> Result<StaticKeyPair, SvcError> {
    let path = key_path(dir);
    if let Some(text) = read(&path) {
        match serde_json::from_str::<KeyFile>(&text) {
            Ok(file) => return Ok(file.key),
            Err(error) => log::warn!(
                "{} is not valid ({error}); making a new host key, clients must pair again",
                path.display()
            ),
        }
    }
    let key = new_key()?;
    json_file::write_atomic(
        &path,
        &KeyFile {
            version: FILE_VERSION,
            key: key.clone(),
        },
    )?;
    log::info!("new host key {}", key.public);
    Ok(key)
}

/// Paired clients; empty (with a warning) when the file is missing or not valid.
pub fn load_paired(dir: &Path) -> Vec<PairedDevice> {
    let path = paired_path(dir);
    let Some(text) = read(&path) else {
        return Vec::new();
    };
    match serde_json::from_str::<PairedFile>(&text) {
        Ok(file) => file.devices,
        Err(error) => {
            log::warn!(
                "{} is not valid ({error}); no paired clients",
                path.display()
            );
            Vec::new()
        }
    }
}

pub fn save_paired(dir: &Path, devices: &[PairedDevice]) -> Result<(), SvcError> {
    json_file::write_atomic(
        &paired_path(dir),
        &PairedFile {
            version: FILE_VERSION,
            devices: devices.to_vec(),
        },
    )
}

#[cfg(test)]
#[path = "secrets_tests.rs"]
mod tests;
