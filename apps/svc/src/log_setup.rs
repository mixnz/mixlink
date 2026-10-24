//! Logging: stderr in console mode, a file under ProgramData when run by the service manager.

use std::fs::{self, OpenOptions};
use std::path::PathBuf;

use env_logger::{Builder, Env, Target};

use crate::error::SvcError;

pub fn init_console_log() {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
}

/// Appends to `%ProgramData%\MixLink\logs\svc.log`; returns the file path.
pub fn init_file_log() -> Result<PathBuf, SvcError> {
    let base = std::env::var_os("ProgramData").unwrap_or_else(|| r"C:\ProgramData".into());
    let dir = PathBuf::from(base).join("MixLink").join("logs");
    fs::create_dir_all(&dir)?;
    let path = dir.join("svc.log");
    let file = OpenOptions::new().create(true).append(true).open(&path)?;
    Builder::from_env(Env::default().default_filter_or("info"))
        .target(Target::Pipe(Box::new(file)))
        .init();
    Ok(path)
}
