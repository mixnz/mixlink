//! Window lifecycle: one window, presented by the video thread, fed by a file (play) or the host
//! (connect, spec 0020), reporting to the launcher.

use std::io;
use std::sync::Arc;

use mixlink_core::ipc::{CloseReason, IPC_VERSION, LatencySummary, StreamToLauncher};
use mixlink_proto::control::CodecId;
use mixlink_stream::args::Mode;
use mixlink_stream::au_queue::{AU_CAPACITY, AU_SLOTS, AuSender, channel};
use mixlink_stream::feeder::Feeder;
use mixlink_stream::video::{VideoEvent, VideoSource, VideoThread};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoopProxy};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Fullscreen, Window, WindowId};

use crate::connect::{self, NetThread};
use crate::ipc::{NetEvent, StreamEvent, send};
use crate::platform::window::any_thread;
use crate::strings;

#[derive(Debug)]
pub struct StreamApp {
    proxy: EventLoopProxy<StreamEvent>,
    mode: Option<Mode>,
    window: Option<Arc<Window>>,
    video: Option<VideoThread>,
    /// Play mode: the queue's sending end until the feeder starts.
    feed: Option<AuSender>,
    feeder: Option<Feeder>,
    net: Option<NetThread>,
    ready_sent: bool,
    closing: bool,
}

impl StreamApp {
    pub fn new(proxy: EventLoopProxy<StreamEvent>, mode: Option<Mode>) -> Self {
        Self {
            proxy,
            mode,
            window: None,
            video: None,
            feed: None,
            feeder: None,
            net: None,
            ready_sent: false,
            closing: false,
        }
    }

    fn open_window(&mut self, event_loop: &ActiveEventLoop) -> Result<(), CloseReason> {
        let error = |message: String| CloseReason::Error { message };
        let attributes = Window::default_attributes()
            .with_title(strings::WINDOW_TITLE)
            .with_inner_size(LogicalSize::new(1280.0, 720.0));
        let window = Arc::new(
            event_loop
                .create_window(attributes)
                .map_err(|e| error(e.to_string()))?,
        );
        let size = window.inner_size();
        let source = match &self.mode {
            Some(Mode::Play(play)) => {
                let (tx, rx) = channel(AU_SLOTS, AU_CAPACITY);
                // The feeder starts once the decoder is open (first present): AUs sent earlier
                // would fill the queue and the first keyframe would be lost.
                self.feed = Some(tx);
                Some(VideoSource::play(rx, play.codec, play.measure))
            }
            Some(Mode::Connect(args)) => {
                self.send(&StreamToLauncher::Connecting);
                let (net, source) = connect::start(args, event_loop, &window, self.proxy.clone())?;
                self.net = Some(net);
                Some(source)
            }
            None => None,
        };
        let proxy = self.proxy.clone();
        let video = VideoThread::start(
            Arc::new(any_thread(&window)),
            (size.width, size.height),
            source,
            move |event| {
                // Fails only after the loop has exited; nothing left to notify.
                let _ = proxy.send_event(StreamEvent::Video(event));
            },
        )
        .map_err(|e| error(e.to_string()))?;
        self.video = Some(video);
        self.window = Some(window);
        Ok(())
    }

    fn on_video(&mut self, event_loop: &ActiveEventLoop, event: VideoEvent) {
        match event {
            VideoEvent::FirstPresent => {
                if self.ready_sent {
                    return;
                }
                self.ready_sent = true;
                self.send(&StreamToLauncher::Ready {
                    ipc_version: IPC_VERSION,
                });
                if let Err(message) = self.start_feeder() {
                    log::error!("stream window error: {message}");
                    self.close(event_loop, CloseReason::Error { message });
                }
            }
            VideoEvent::Failed(message) => {
                self.close(event_loop, CloseReason::Error { message });
            }
        }
    }

    fn on_net(&mut self, event_loop: &ActiveEventLoop, event: NetEvent) {
        match event {
            NetEvent::Connected { mode, codec } => {
                log::info!(
                    "connected: {}x{} at {} mHz, {codec:?}",
                    mode.width,
                    mode.height,
                    mode.refresh_mhz
                );
                self.send(&StreamToLauncher::Connected {
                    width: mode.width,
                    height: mode.height,
                    refresh_mhz: mode.refresh_mhz,
                    codec: match codec {
                        CodecId::H264 => "h264",
                        CodecId::Hevc => "hevc",
                    }
                    .to_owned(),
                });
            }
            NetEvent::Ended(reason) => self.close(event_loop, reason),
        }
    }

    fn start_feeder(&mut self) -> Result<(), String> {
        if let (Some(Mode::Play(play)), Some(tx)) = (&self.mode, self.feed.take()) {
            let feeder =
                Feeder::start(play, tx).map_err(|e| format!("{}: {e}", play.path.display()))?;
            self.feeder = Some(feeder);
        }
        Ok(())
    }

    /// Stops the network, the feeder and the video thread, then tells the launcher why we close (it
    /// may already be gone) and stops the loop.
    fn close(&mut self, event_loop: &ActiveEventLoop, reason: CloseReason) {
        if self.closing {
            return;
        }
        self.closing = true;
        log::info!("closing: {reason:?}");
        if let Some(net) = self.net.take()
            && let Some(end) = net.stop()
        {
            log::info!("network: {:?}", end.counts);
        }
        if let Some(feeder) = self.feeder.take() {
            feeder.stop();
        }
        let mut latency: Option<LatencySummary> = None;
        if let Some(stats) = self.video.take().and_then(VideoThread::stop) {
            log::info!("frame stats:\n{}", stats.summary());
            if matches!(self.mode, Some(Mode::Connect(_))) {
                latency = Some(stats.latency_summary());
            }
        }
        self.send(&StreamToLauncher::Closed { reason, latency });
        event_loop.exit();
    }

    /// F11: borderless fullscreen on the window's monitor, so Windows can flip frames straight to the
    /// screen instead of composing the window (steadier pacing at 60 Hz, spec 0020).
    fn toggle_fullscreen(&self) {
        if let Some(window) = &self.window {
            let next = match window.fullscreen() {
                Some(_) => None,
                None => Some(Fullscreen::Borderless(None)),
            };
            window.set_fullscreen(next);
        }
    }

    fn send(&self, msg: &StreamToLauncher) {
        if let Err(error) = send(&mut io::stdout().lock(), msg) {
            log::debug!("launcher did not get {msg:?}: {error}");
        }
    }
}

impl ApplicationHandler<StreamEvent> for StreamApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none()
            && let Err(reason) = self.open_window(event_loop)
        {
            log::error!("stream window error: {reason:?}");
            self.close(event_loop, reason);
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => self.close(event_loop, CloseReason::User),
            WindowEvent::Resized(size) => {
                if let Some(video) = &self.video {
                    video.resize(size.width, size.height);
                }
            }
            WindowEvent::KeyboardInput { event, .. }
                if event.state == ElementState::Pressed
                    && !event.repeat
                    && event.physical_key == PhysicalKey::Code(KeyCode::F11) =>
            {
                self.toggle_fullscreen();
            }
            WindowEvent::RedrawRequested => {
                if let (Some(video), Some(window)) = (&self.video, &self.window) {
                    let size = window.inner_size();
                    video.resize(size.width, size.height);
                }
            }
            _ => {}
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: StreamEvent) {
        match event {
            StreamEvent::Stop => self.close(event_loop, CloseReason::Stopped),
            // Play mode runs without a launcher (terminal, Explorer): only the window closes it.
            StreamEvent::LauncherGone if matches!(self.mode, Some(Mode::Play(_))) => {
                log::info!("stdin closed; play mode keeps running until the window is closed");
            }
            StreamEvent::LauncherGone => self.close(event_loop, CloseReason::Stopped),
            StreamEvent::Video(video) => self.on_video(event_loop, video),
            StreamEvent::Net(net) => self.on_net(event_loop, net),
        }
    }
}
