//! The saved screen layout on disk (spec 0015): written once before a switch, deleted after restore.
//! An existing file is never overwritten: it holds the user's real layout from before a crash.

use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use super::topology::{SNAPSHOT_VERSION, Snapshot};
use crate::error::SvcError;

#[derive(Debug, Clone)]
pub struct SnapshotStore {
    path: PathBuf,
}

impl SnapshotStore {
    pub fn new(path: PathBuf) -> SnapshotStore {
        SnapshotStore { path }
    }

    /// `<ProgramData>\MixLink\display\topology.json`.
    pub fn default_path(program_data: &Path) -> PathBuf {
        program_data
            .join("MixLink")
            .join("display")
            .join("topology.json")
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// `None` when there is no file. A file that cannot be used (cut short by a power loss, another
    /// version) is logged and deleted, so it never blocks a start.
    pub fn load(&self) -> Option<Snapshot> {
        let text = match std::fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(error) if error.kind() == ErrorKind::NotFound => return None,
            Err(error) => {
                log::warn!("cannot read {}: {error}", self.path.display());
                return None;
            }
        };
        match serde_json::from_str::<Snapshot>(&text) {
            Ok(snapshot) if snapshot.version == SNAPSHOT_VERSION => Some(snapshot),
            Ok(snapshot) => {
                log::warn!("saved layout has version {}; ignored", snapshot.version);
                self.discard();
                None
            }
            Err(error) => {
                log::warn!("saved layout is not valid ({error}); ignored");
                self.discard();
                None
            }
        }
    }

    /// Writes `snapshot` unless a file is already there (returns `false` then). Atomic: a `.tmp`
    /// file is renamed over the target.
    pub fn save_if_absent(&self, snapshot: &Snapshot) -> Result<bool, SvcError> {
        if self.path.exists() {
            return Ok(false);
        }
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = serde_json::to_string_pretty(snapshot).map_err(std::io::Error::other)?;
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, &self.path)?;
        Ok(true)
    }

    pub fn delete(&self) -> Result<(), SvcError> {
        match std::fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    fn discard(&self) {
        if let Err(error) = self.delete() {
            log::warn!("cannot delete {}: {error}", self.path.display());
        }
    }
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
