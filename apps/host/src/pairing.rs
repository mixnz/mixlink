//! Pairing in the tray (spec 0018): the PIN prompt the service reports, paired devices, Remove.

use mixlink_core::ids::DeviceId;
use mixlink_core::svc_ipc::{PIPE_NAME, PairedInfo, PairingPrompt, SvcPipe, SvcPipeError};
use serde::Serialize;

/// The prompt as the UI gets it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptState {
    pub device_name: String,
    pub pin: String,
    pub expires_s: u32,
}

impl From<PairingPrompt> for PromptState {
    fn from(prompt: PairingPrompt) -> PromptState {
        PromptState {
            device_name: prompt.device_name,
            pin: prompt.pin,
            expires_s: prompt.expires_s,
        }
    }
}

/// Same prompt for the UI: the countdown alone is not a change.
pub fn same_prompt(a: Option<&PromptState>, b: Option<&PromptState>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => a.pin == b.pin && a.device_name == b.device_name,
        (None, None) => true,
        _ => false,
    }
}

/// A new prompt brings the window to the front, so the user sees the PIN.
pub fn should_show(old: Option<&PromptState>, new: Option<&PromptState>) -> bool {
    new.is_some() && !same_prompt(old, new)
}

/// One paired device as the UI shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairedRow {
    pub device: String,
    pub name: String,
    /// Unix seconds.
    pub paired_at: u64,
}

pub fn rows(devices: Vec<PairedInfo>) -> Vec<PairedRow> {
    devices
        .into_iter()
        .map(|info| PairedRow {
            device: info.device.to_string(),
            name: info.name,
            paired_at: info.paired_at,
        })
        .collect()
}

/// Why a pairing request from the UI failed (shown as text).
#[derive(Debug, thiserror::Error)]
pub enum TrayPairError {
    #[error(transparent)]
    Pipe(#[from] SvcPipeError),
    #[error("the pipe is not served by the MixLink service")]
    Untrusted,
    #[error("bad device id: {0}")]
    Device(String),
}

impl Serialize for TrayPairError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

/// A short connection to the service for one request; the server must be the service.
fn with_pipe<T>(
    f: impl FnOnce(&mut SvcPipe) -> Result<T, SvcPipeError>,
) -> Result<T, TrayPairError> {
    let (mut pipe, _) = SvcPipe::connect(PIPE_NAME)?;
    if !trusted(&pipe) {
        return Err(TrayPairError::Untrusted);
    }
    Ok(f(&mut pipe)?)
}

/// The pipe server is the MixLink service (spec 0018); always false off Windows.
pub fn trusted(pipe: &SvcPipe) -> bool {
    #[cfg(windows)]
    {
        match crate::platform::windows::pipe_owner::server_is_service(pipe.file()) {
            Ok(trusted) => trusted,
            Err(error) => {
                log::warn!("cannot check the service pipe owner: {error}");
                false
            }
        }
    }
    #[cfg(not(windows))]
    {
        let _ = pipe;
        false
    }
}

pub fn cancel() -> Result<(), TrayPairError> {
    with_pipe(SvcPipe::cancel_pairing)
}

pub fn list() -> Result<Vec<PairedRow>, TrayPairError> {
    with_pipe(SvcPipe::list_paired).map(rows)
}

pub fn remove(device: &str) -> Result<(), TrayPairError> {
    let device: DeviceId = device
        .parse()
        .map_err(|_| TrayPairError::Device(device.to_owned()))?;
    with_pipe(|pipe| pipe.remove_paired(device))
}

#[cfg(test)]
#[path = "pairing_tests.rs"]
mod tests;
