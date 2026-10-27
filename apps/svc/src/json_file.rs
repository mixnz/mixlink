//! Small JSON state files under `%ProgramData%\MixLink`: written to a temporary file, then renamed.

use std::path::Path;

use serde::Serialize;

use crate::error::SvcError;

/// Pretty JSON with a trailing newline; the old file stays whole until the rename.
pub fn write_atomic<T: Serialize>(path: &Path, value: &T) -> Result<(), SvcError> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut text = serde_json::to_string_pretty(value).map_err(std::io::Error::other)?;
    text.push('\n');
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}
