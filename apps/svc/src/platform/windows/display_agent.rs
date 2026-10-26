//! Both ends of the display helper (spec 0015). The service writes one request file, runs
//! `mixlink-svc display-agent --in <file> --out <file>` in the console session and reads the
//! reply file. Files live in the SYSTEM + Administrators folder of the layout snapshot.

use std::path::{Path, PathBuf};
use std::time::Duration;

use windows::Win32::System::StationsAndDesktops::{
    DESKTOP_ACCESS_FLAGS, DESKTOP_CONTROL_FLAGS, DESKTOP_READOBJECTS, DESKTOP_SWITCHDESKTOP,
    DESKTOP_WRITEOBJECTS, OpenInputDesktop, SetThreadDesktop,
};

use super::console_process::run_in_console_session;
use super::display_config::CcdConfig;
use crate::display::agent::{AgentReply, AgentRequest, AgentRunner, handle};
use crate::error::SvcError;

/// A layout change takes about a second; a helper that is still running after this is stuck.
const HELPER_TIMEOUT: Duration = Duration::from_secs(10);

/// Service side: one helper process per request.
#[derive(Debug)]
pub struct ProcessRunner {
    exe: PathBuf,
    dir: PathBuf,
    next: u32,
}

impl ProcessRunner {
    /// `dir` must already be limited to SYSTEM + Administrators (the helper runs as SYSTEM).
    pub fn new(dir: PathBuf) -> Result<ProcessRunner, SvcError> {
        Ok(ProcessRunner {
            exe: std::env::current_exe()?,
            dir,
            next: 0,
        })
    }
}

fn json_error(error: serde_json::Error) -> SvcError {
    SvcError::Io(std::io::Error::other(error))
}

impl AgentRunner for ProcessRunner {
    fn call(&mut self, request: &AgentRequest) -> Result<AgentReply, SvcError> {
        self.next = self.next.wrapping_add(1);
        let stem = format!("agent-{}-{}", std::process::id(), self.next);
        let input = self.dir.join(format!("{stem}-in.json"));
        let output = self.dir.join(format!("{stem}-out.json"));
        std::fs::write(&input, serde_json::to_vec(request).map_err(json_error)?)?;
        let command = format!(
            "\"{}\" display-agent --in \"{}\" --out \"{}\"",
            self.exe.display(),
            input.display(),
            output.display()
        );
        let code = run_in_console_session(&command, HELPER_TIMEOUT);
        let reply = std::fs::read(&output);
        let _ = std::fs::remove_file(&input);
        let _ = std::fs::remove_file(&output);
        let code = code?;
        if code != 0 {
            return Err(SvcError::DisplayAgent(format!(
                "the display helper stopped with code {code:#x}"
            )));
        }
        serde_json::from_slice(&reply?).map_err(json_error)
    }
}

/// Helper side: answer the request in `input` with the CCD API, reply into `output`.
pub fn serve_one(input: &Path, output: &Path) -> Result<(), SvcError> {
    attach_input_desktop();
    let request: AgentRequest =
        serde_json::from_slice(&std::fs::read(input)?).map_err(json_error)?;
    let reply = handle(&mut CcdConfig, request);
    std::fs::write(output, serde_json::to_vec(&reply).map_err(json_error)?)?;
    Ok(())
}

/// The display config API works on the desktop that gets input: the user's, or Winlogon's at
/// the login and lock screens. Failure is left to the API calls that follow.
fn attach_input_desktop() {
    let access = DESKTOP_ACCESS_FLAGS(
        DESKTOP_READOBJECTS.0 | DESKTOP_WRITEOBJECTS.0 | DESKTOP_SWITCHDESKTOP.0,
    );
    // SAFETY: plain open of the input desktop; the handle stays open for the process lifetime.
    match unsafe { OpenInputDesktop(DESKTOP_CONTROL_FLAGS(0), false, access) } {
        Ok(desktop) => {
            // SAFETY: fresh process, this thread owns no windows or hooks yet.
            if let Err(error) = unsafe { SetThreadDesktop(desktop) } {
                log::warn!("SetThreadDesktop: {error}");
            }
        }
        Err(error) => log::warn!("OpenInputDesktop: {error}"),
    }
}
