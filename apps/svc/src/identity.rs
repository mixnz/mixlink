//! The host's stable id (spec 0017): `%ProgramData%\MixLink\identity.json`, made on first start.
//! Not user config, so it stays out of `host.json`.

use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use mixlink_core::ids::DeviceId;
use serde::{Deserialize, Serialize};

use crate::error::SvcError;

const IDENTITY_VERSION: u32 = 1;

#[derive(Debug, Serialize, Deserialize)]
struct IdentityFile {
    version: u32,
    id: DeviceId,
}

pub fn default_path(program_data: &Path) -> PathBuf {
    program_data.join("MixLink").join("identity.json")
}

/// The saved id; a new one from `new_id` (saved) when the file is missing or not usable.
pub fn load_or_create(
    path: &Path,
    new_id: impl FnOnce() -> Result<DeviceId, SvcError>,
) -> Result<DeviceId, SvcError> {
    match std::fs::read_to_string(path) {
        Ok(text) => match serde_json::from_str::<IdentityFile>(&text) {
            Ok(file) => return Ok(file.id),
            Err(error) => log::warn!(
                "{} is not valid ({error}); making a new host id",
                path.display()
            ),
        },
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => log::warn!(
            "cannot read {}: {error}; making a new host id",
            path.display()
        ),
    }
    let id = new_id()?;
    save(path, id)?;
    log::info!("new host id {id}");
    Ok(id)
}

/// Writes a temporary file, then renames it over the old one.
fn save(path: &Path, id: DeviceId) -> Result<(), SvcError> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let file = IdentityFile {
        version: IDENTITY_VERSION,
        id,
    };
    let mut text = serde_json::to_string_pretty(&file).map_err(std::io::Error::other)?;
    text.push('\n');
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
#[path = "identity_tests.rs"]
mod tests;
