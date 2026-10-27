//! `capture try` (spec 0019): a virtual monitor recorded through the frame share and NVENC.

pub mod flow;
pub mod report;
#[cfg(windows)]
mod sink;
pub mod writer;

#[cfg(windows)]
pub use self::windows::run;

#[cfg(windows)]
mod windows {
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use mixlink_capture::IddSource;
    use mixlink_codec::EncoderConfig;

    use super::flow::run_loop;
    use super::report::{Stats, expected_frames};
    use super::sink::NvencSink;
    use super::writer::{Writer, WriterTx};
    use crate::args::TryTopology;
    use crate::capture_args::{CaptureCmd, CaptureTry};
    use crate::display::ports::DisplayDriver;
    use crate::display::service::secured_store;
    use crate::display::session::{self, Ports};
    use crate::error::SvcError;
    use crate::platform::windows::console::install_ctrl_handler;
    use crate::platform::windows::display_config::CcdConfig;
    use crate::platform::windows::install::is_running;
    use crate::platform::windows::priority::apply_thread;
    use crate::platform::windows::vdisplay::VDisplay;
    use crate::stop::StopSignal;
    use crate::strings;

    pub fn run(cmd: &CaptureCmd) -> Result<(), SvcError> {
        match cmd {
            CaptureCmd::Try(cmd) => try_capture(cmd),
        }
    }

    /// Monitor on, record, monitor off (also after an error or Ctrl+C). Refused while the service runs.
    fn try_capture(cmd: &CaptureTry) -> Result<(), SvcError> {
        if is_running()? {
            return Err(SvcError::ServiceRunning);
        }
        let stop = Arc::new(StopSignal::new());
        install_ctrl_handler(Arc::clone(&stop))?;
        let mut driver = VDisplay::open()?;
        let mut config = CcdConfig;
        let store = secured_store()?;
        let mut ports = Ports {
            driver: &mut driver,
            config: &mut config,
            store: &store,
        };
        let topology = match cmd.topology {
            TryTopology::Keep => None,
            TryTopology::Apply(topology) => Some(topology),
        };
        let mut headless = None;
        let active = session::begin(
            &mut ports,
            &cmd.client,
            &cmd.mode,
            topology,
            &mut headless,
            &std::thread::sleep,
        )?;
        let recorded = connector_of(ports.driver, active.monitor).and_then(|connector| {
            eprintln!("{}", strings::capture_started(cmd));
            record(cmd, connector, &stop)
        });
        session::end(&mut ports, active, &mut headless)?;
        eprintln!("{}", strings::MONITOR_OFF);
        eprintln!("{}", strings::capture_report(&recorded?, &cmd.out));
        Ok(())
    }

    fn connector_of(driver: &mut VDisplay, monitor: u32) -> Result<u32, SvcError> {
        driver
            .query()?
            .monitors
            .iter()
            .find(|m| m.id == monitor)
            .map(|m| m.connector)
            .ok_or(SvcError::DisplayNotReady)
    }

    /// Writer thread + thread `cap-frames` (wait, encode, queue: one thread, the driver's hold makes
    /// it lockstep). Returns once the time is up, Ctrl+C, or an error.
    fn record(cmd: &CaptureTry, connector: u32, stop: &Arc<StopSignal>) -> Result<Stats, SvcError> {
        let (tx, writer) = Writer::spawn(&cmd.out)?;
        let config = EncoderConfig {
            codec: cmd.codec,
            width: cmd.mode.width,
            height: cmd.mode.height,
            fps: cmd.mode.refresh_mhz.div_ceil(1000).max(1),
            bitrate_kbps: cmd.bitrate_kbps,
        };
        let frames = expected_frames(cmd.seconds, cmd.mode.refresh_mhz);
        let deadline = Duration::from_secs(cmd.seconds.into());
        let (levers, stop) = (cmd.levers, Arc::clone(stop));
        let capture = std::thread::Builder::new()
            .name("cap-frames".into())
            .spawn(move || {
                let _levers = apply_thread(levers);
                frame_loop(connector, config, tx, levers.gpu, frames, deadline, &stop)
            })?;
        let stats = capture.join().unwrap_or_else(|_| {
            Err(SvcError::Io(std::io::Error::other(
                "capture thread panicked",
            )))
        });
        let written = writer
            .join()
            .unwrap_or_else(|_| Err(std::io::Error::other("writer thread panicked")));
        let stats = stats?;
        written?;
        Ok(stats)
    }

    fn frame_loop(
        connector: u32,
        config: EncoderConfig,
        tx: WriterTx,
        gpu_lever: bool,
        frames: usize,
        duration: Duration,
        stop: &StopSignal,
    ) -> Result<Stats, SvcError> {
        let mut src = IddSource::open(connector)?;
        let mut sink = NvencSink::new(config, tx, gpu_lever);
        let mut stats = Stats::with_capacity(frames);
        let deadline = Instant::now() + duration;
        run_loop(
            &mut src,
            &mut sink,
            &|| stop.is_stopped(),
            deadline,
            &mut stats,
        )?;
        stats.hold_timeouts = src.hold_timeouts();
        stats.ring_fallbacks = src.ring_fallbacks();
        Ok(stats)
    }
}
