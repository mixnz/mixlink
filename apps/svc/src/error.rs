use std::io;
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum SvcError {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[cfg(windows)]
    #[error("{call} failed: {source}")]
    Win32 {
        call: &'static str,
        #[source]
        source: windows::core::Error,
    },
    #[error("another process already owns the service pipe")]
    PipeTaken,
    #[error("This needs administrator rights. Run it from an administrator terminal.")]
    NotAdmin,
    #[error("the MixLink service did not stop within 10 seconds")]
    StopTimeout,
    #[error("{0}")]
    Usage(String),
    #[error(
        "Driver package not found at {}. Build it with drivers/build.ps1 and pass --from <folder>.",
        path.display()
    )]
    PackageMissing { path: PathBuf },
    #[error("saved driver state is not valid: {0}")]
    StateInvalid(String),
    #[error("driver {step} failed: {detail}")]
    Driver { step: &'static str, detail: String },
    #[error("the signing key still exists after deletion")]
    KeyNotDeleted,
}

#[cfg(windows)]
impl SvcError {
    pub fn win32(call: &'static str) -> impl FnOnce(windows::core::Error) -> SvcError {
        move |source| SvcError::Win32 { call, source }
    }
}
