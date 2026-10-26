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
    #[error(
        "The MixLink virtual display driver isn't installed. Install it with `mixlink-svc drivers install`."
    )]
    DisplayDriverMissing,
    #[error("The MixLink virtual display is still starting. Try again in a few seconds.")]
    DisplayNotReady,
    #[error(transparent)]
    Wire(#[from] crate::display::wire::WireError),
    #[error("The new MixLink monitor didn't show up in Windows within 5 seconds.")]
    MonitorTimeout,
    #[error("The MixLink service is running. Stop it first with `sc stop MixLink`.")]
    ServiceRunning,
    #[error("display helper: {0}")]
    DisplayAgent(String),
    #[error("screen layout {step} failed: Windows error {code}")]
    Topology { step: &'static str, code: i32 },
    /// Uninstall stopped: a program holds a device open (`Some` when Windows names it).
    #[error("{}", crate::strings::drivers_held(.0.as_deref()))]
    DeviceHeld(Option<String>),
    #[error(
        "The MixLink virtual input driver isn't installed. Install it with `mixlink-svc drivers install`."
    )]
    InputDriverMissing,
    #[error(
        "MixLink input is in use by the MixLink service. Stop it first with `sc stop MixLink`."
    )]
    InputInUse,
    #[error(
        "MixLink virtual input isn't ready: its control device can't reach the input device. Reinstall the drivers."
    )]
    InputNotPaired,
    #[cfg(windows)]
    #[error(
        "MixLink virtual input driver doesn't match this program ({0}). Reinstall the drivers."
    )]
    InputWire(mixlink_input::platform::windows::vhid_wire::WireError),
}

#[cfg(windows)]
impl SvcError {
    pub fn win32(call: &'static str) -> impl FnOnce(windows::core::Error) -> SvcError {
        move |source| SvcError::Win32 { call, source }
    }
}

#[cfg(windows)]
impl From<mixlink_input::platform::windows::vhid::VHidError> for SvcError {
    fn from(error: mixlink_input::platform::windows::vhid::VHidError) -> SvcError {
        use mixlink_input::platform::windows::vhid::VHidError;
        match error {
            VHidError::Missing => SvcError::InputDriverMissing,
            VHidError::AccessDenied => SvcError::NotAdmin,
            VHidError::InUse => SvcError::InputInUse,
            VHidError::NotPaired => SvcError::InputNotPaired,
            VHidError::Wire(wire) => SvcError::InputWire(wire),
            VHidError::Win32 { call, source } => SvcError::Win32 { call, source },
        }
    }
}
