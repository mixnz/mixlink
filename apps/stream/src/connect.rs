//! Connect mode (spec 0020): the `Hello` from this PC's monitors, the keys from the client store, and
//! the `stream-net` thread that feeds the video thread.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, OnceLock};
use std::thread::{self, JoinHandle};

use mixlink_core::config::computer_name;
use mixlink_core::ipc::CloseReason;
use mixlink_net::client_store;
use mixlink_proto::control::{CodecId, ControlMessage, DisplayInfo};
use mixlink_proto::header::WIRE_VERSION;
use mixlink_stream::args::ConnectArgs;
use mixlink_stream::au_queue::{AU_CAPACITY, AU_SLOTS, channel};
use mixlink_stream::input::sender::InputTx;
use mixlink_stream::net::cursor_state::cursor_channel;
use mixlink_stream::net::session::SILENCE;
use mixlink_stream::net::{SessionEnd, SessionIo, SessionPlan, run};
use mixlink_stream::video::VideoSource;
use winit::event_loop::{ActiveEventLoop, EventLoopProxy};
use winit::monitor::MonitorHandle;
use winit::window::Window;

use crate::ipc::{NetEvent, StreamEvent};
use crate::strings;

/// Displays a `Hello` lists at most.
const MAX_DISPLAYS: usize = 8;

/// The running `stream-net` thread.
#[derive(Debug)]
pub struct NetThread {
    stop: Arc<AtomicBool>,
    handle: JoinHandle<SessionEnd>,
    /// The input sender, sent once the session is up (spec 0021).
    input: Receiver<InputTx>,
}

impl NetThread {
    /// The input sender once `Connected` came (it is sent just before).
    pub fn take_input(&self) -> Option<InputTx> {
        self.input.try_recv().ok()
    }

    /// Stops it (it sends `Bye` when the session was up) and waits.
    pub fn stop(self) -> Option<SessionEnd> {
        self.stop.store(true, Ordering::Release);
        self.handle.thread().unpark();
        self.handle.join().ok()
    }
}

/// Starts the session thread; returns it and the video source it feeds.
pub fn start(
    args: &ConnectArgs,
    event_loop: &ActiveEventLoop,
    window: &Window,
    proxy: EventLoopProxy<StreamEvent>,
) -> Result<(NetThread, VideoSource), CloseReason> {
    let error = |message: String| CloseReason::Error { message };
    let dir =
        client_store::default_dir().ok_or_else(|| error(strings::NO_PROFILE_FOLDER.to_owned()))?;
    let me = client_store::load_or_create_identity(&dir).map_err(|e| error(e.to_string()))?;
    let Some(host) = client_store::load_known(&dir)
        .into_iter()
        .find(|known| known.host == args.host)
    else {
        return Err(CloseReason::NotPaired);
    };
    let hello = hello(me.id, event_loop, window);
    let plan = SessionPlan {
        me,
        host,
        addr: args.addr,
        hello,
        drop: args.drop,
        silence: SILENCE,
    };
    let (au, rx) = channel(AU_SLOTS, AU_CAPACITY);
    let (cursor, cursor_rx) = cursor_channel();
    let codec = Arc::new(OnceLock::new());
    let want_keyframe = Arc::new(AtomicBool::new(false));
    let stop = Arc::new(AtomicBool::new(false));
    let (input, input_rx) = mpsc::sync_channel(1);
    let io = SessionIo {
        au,
        cursor,
        codec: Arc::clone(&codec),
        want_keyframe: Arc::clone(&want_keyframe),
        stop: Arc::clone(&stop),
        input,
    };
    let host_id = args.host;
    let handle = thread::Builder::new()
        .name("stream-net".into())
        .spawn(move || {
            let _mmcss = mixlink_core::mmcss::Mmcss::enter(mixlink_core::mmcss::MmcssTask::Games);
            let connected = proxy.clone();
            let end = run(&plan, io, |mode, codec| {
                // Fails only after the loop has exited; nothing left to notify.
                let _ = connected.send_event(StreamEvent::Net(NetEvent::Connected { mode, codec }));
            });
            if end.reason == Some(CloseReason::NotPaired)
                && let Err(error) = client_store::forget(&dir, host_id)
            {
                log::warn!("host not forgotten here: {error}");
            }
            if let Some(reason) = end.reason.clone() {
                let _ = proxy.send_event(StreamEvent::Net(NetEvent::Ended(reason)));
            }
            end
        })
        .map_err(|e| error(e.to_string()))?;
    let source = VideoSource {
        rx,
        codec,
        measure: false,
        cursor: Some(cursor_rx),
        want_keyframe,
    };
    Ok((
        NetThread {
            stop,
            handle,
            input: input_rx,
        },
        source,
    ))
}

/// This PC's displays; `stream_display` = the one the window is on.
fn hello(
    device: mixlink_core::ids::DeviceId,
    event_loop: &ActiveEventLoop,
    window: &Window,
) -> ControlMessage {
    let monitors: Vec<MonitorHandle> = event_loop.available_monitors().take(MAX_DISPLAYS).collect();
    let current = window.current_monitor();
    let mut displays: Vec<DisplayInfo> = monitors.iter().map(display_info).collect();
    let mut stream_display = current
        .as_ref()
        .and_then(|cur| monitors.iter().position(|m| m == cur))
        .unwrap_or(0);
    if displays.is_empty() {
        // No monitor reported (remote desktop, headless test): ask for 1080p60.
        displays.push(DisplayInfo {
            width: 1920,
            height: 1080,
            refresh_mhz: 60_000,
            hdr: false,
            dpi: 96,
        });
        stream_display = 0;
    }
    ControlMessage::Hello {
        proto: WIRE_VERSION,
        device,
        name: computer_name(strings::DEVICE_NAME),
        displays,
        stream_display: u8::try_from(stream_display).unwrap_or(0),
        codecs: vec![CodecId::H264, CodecId::Hevc],
    }
}

fn display_info(monitor: &MonitorHandle) -> DisplayInfo {
    let size = monitor.size();
    DisplayInfo {
        width: size.width.max(1),
        height: size.height.max(1),
        refresh_mhz: monitor
            .refresh_rate_millihertz()
            .unwrap_or(60_000)
            .clamp(1_000, 1_000_000),
        hdr: false,
        dpi: (monitor.scale_factor() * 96.0)
            .round()
            .clamp(1.0, f64::from(u16::MAX)) as u16,
    }
}
