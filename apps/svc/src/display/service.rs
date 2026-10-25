//! The virtual display in the service (spec 0015): at start, open the driver, restore a layout a
//! crash left switched, add the headless monitor when no physical monitor is active, then hold the
//! driver handle until stop (closing it removes our monitors).

use std::io;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use mixlink_core::config::DisplaySettings;

use super::session::{self, Ports};
use super::store::SnapshotStore;
use crate::error::SvcError;
use crate::platform::windows::dir_security::secure_dir;
use crate::platform::windows::display_config::CcdConfig;
use crate::platform::windows::drivers_host::program_data;
use crate::platform::windows::vdisplay::VDisplay;
use crate::stop::StopSignal;

/// Auto-start can run before the driver's adapter is up: retry this often, this long.
const OPEN_RETRY: Duration = Duration::from_millis(500);
const OPEN_FOR: Duration = Duration::from_secs(30);

/// `%ProgramData%\MixLink\display\topology.json`, not touched on disk.
pub fn default_store() -> SnapshotStore {
    SnapshotStore::new(SnapshotStore::default_path(&program_data()))
}

/// As `default_store`, with its folder created and limited to SYSTEM + Administrators.
pub fn secured_store() -> Result<SnapshotStore, SvcError> {
    let store = default_store();
    if let Some(dir) = store.path().parent() {
        secure_dir(dir)?;
    }
    Ok(store)
}

/// Runs the display start-up on its own thread; it never fails the service.
pub fn spawn(stop: Arc<StopSignal>, settings: DisplaySettings) -> io::Result<JoinHandle<()>> {
    std::thread::Builder::new()
        .name("svc-display".into())
        .spawn(move || run(&stop, settings))
}

fn run(stop: &StopSignal, settings: DisplaySettings) {
    log::info!("virtual display: topology setting {:?}", settings.topology);
    let deadline = Instant::now() + OPEN_FOR;
    let mut headless = None;
    let driver = loop {
        match start(&mut headless) {
            Ok(driver) => break driver,
            Err(SvcError::DisplayDriverMissing | SvcError::DisplayNotReady)
                if Instant::now() < deadline =>
            {
                if stop.wait_timeout(OPEN_RETRY) {
                    return;
                }
            }
            Err(error) => {
                log::warn!("virtual display not started: {error}");
                return;
            }
        }
    };
    log::info!("virtual display ready (headless monitor: {headless:?})");
    stop.wait();
    drop(driver);
}

fn start(headless: &mut Option<u32>) -> Result<VDisplay, SvcError> {
    let mut driver = VDisplay::open()?;
    let store = secured_store()?;
    let mut config = CcdConfig;
    let mut ports = Ports {
        driver: &mut driver,
        config: &mut config,
        store: &store,
    };
    match session::start_up(&mut ports, headless) {
        Ok(()) => {}
        Err(SvcError::DisplayNotReady) => return Err(SvcError::DisplayNotReady),
        // The layout part failed (logged); keep the driver so monitors added so far stay.
        Err(error) => log::warn!("virtual display start-up: {error}"),
    }
    Ok(driver)
}
