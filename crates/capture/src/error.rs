//! Errors of the capture crate.

#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    #[error(
        "no frame share for monitor connector {connector} (driver missing, or no monitor there)"
    )]
    NoFrameShare { connector: u32 },
    #[error("the display driver shares frames in format version {found}; this build reads another")]
    WrongVersion { found: u32 },
    #[error("the virtual monitor stopped sharing frames")]
    NoSwapchain,
    #[cfg(windows)]
    #[error("{context} failed: {source}")]
    Win32 {
        context: &'static str,
        source: windows::core::Error,
    },
}
