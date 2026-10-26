//! The host config file the service reads at start (spec 0015): `%ProgramData%\MixLink\host.json`.

use std::io::ErrorKind;
use std::path::{Path, PathBuf};

pub use mixlink_core::config::HostConfig;

pub fn default_path(program_data: &Path) -> PathBuf {
    program_data.join("MixLink").join("host.json")
}

/// The config at `path`; defaults when there is no file, or (logged) when it cannot be used.
pub fn load(path: &Path) -> HostConfig {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == ErrorKind::NotFound => return HostConfig::default(),
        Err(error) => {
            log::warn!("cannot read {}: {error}; using defaults", path.display());
            return HostConfig::default();
        }
    };
    HostConfig::from_json(&text).unwrap_or_else(|error| {
        log::warn!("{} is not valid ({error}); using defaults", path.display());
        HostConfig::default()
    })
}

#[cfg(test)]
#[path = "host_config_tests.rs"]
mod tests;
