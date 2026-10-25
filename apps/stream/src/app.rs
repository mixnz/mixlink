//! Window lifecycle: one window, presented by the video thread, reporting to the launcher.

use std::io;
use std::sync::Arc;

use mixlink_core::ipc::{CloseReason, IPC_VERSION, StreamToLauncher};
use mixlink_stream::args::PlayArgs;
use mixlink_stream::au_queue::{AU_CAPACITY, AU_SLOTS, AuSender, channel};
use mixlink_stream::feeder::Feeder;
use mixlink_stream::video::{VideoEvent, VideoSource, VideoThread};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoopProxy};
use winit::window::{Window, WindowId};

use crate::ipc::{StreamEvent, send};
use crate::platform::window::any_thread;
use crate::strings;

#[derive(Debug)]
pub struct StreamApp {
    proxy: EventLoopProxy<StreamEvent>,
    play: Option<PlayArgs>,
    window: Option<Arc<Window>>,
    video: Option<VideoThread>,
    /// Play mode: the queue's sending end until the feeder starts.
    feed: Option<AuSender>,
    feeder: Option<Feeder>,
    ready_sent: bool,
    closing: bool,
}

impl StreamApp {
    pub fn new(proxy: EventLoopProxy<StreamEvent>, play: Option<PlayArgs>) -> Self {
        Self {
            proxy,
            play,
            window: None,
            video: None,
            feed: None,
            feeder: None,
            ready_sent: false,
            closing: false,
        }
    }

    fn open_window(&mut self, event_loop: &ActiveEventLoop) -> Result<(), String> {
        let attributes = Window::default_attributes()
            .with_title(strings::WINDOW_TITLE)
            .with_inner_size(LogicalSize::new(1280.0, 720.0));
        let window = Arc::new(
            event_loop
                .create_window(attributes)
                .map_err(|e| e.to_string())?,
        );
        let size = window.inner_size();
        let mut source = None;
        if let Some(play) = &self.play {
            let (tx, rx) = channel(AU_SLOTS, AU_CAPACITY);
            // The feeder starts once the decoder is open (first present): AUs sent earlier
            // would fill the queue and the first keyframe would be lost.
            self.feed = Some(tx);
            source = Some(VideoSource {
                rx,
                codec: play.codec,
                measure: play.measure,
            });
        }
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
        .map_err(|e| e.to_string())?;
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
                let ready = StreamToLauncher::Ready {
                    ipc_version: IPC_VERSION,
                };
                let result = send(&mut io::stdout().lock(), &ready)
                    .map_err(|e| e.to_string())
                    .and_then(|()| self.start_feeder());
                self.close_on_error(event_loop, result);
            }
            VideoEvent::Failed(message) => {
                self.close(event_loop, CloseReason::Error { message });
            }
        }
    }

    fn start_feeder(&mut self) -> Result<(), String> {
        if let (Some(play), Some(tx)) = (&self.play, self.feed.take()) {
            let feeder =
                Feeder::start(play, tx).map_err(|e| format!("{}: {e}", play.path.display()))?;
            self.feeder = Some(feeder);
        }
        Ok(())
    }

    /// Stops the feeder and the video thread, then tells the launcher why we close (it may
    /// already be gone) and stops the loop.
    fn close(&mut self, event_loop: &ActiveEventLoop, reason: CloseReason) {
        if self.closing {
            return;
        }
        self.closing = true;
        log::info!("closing: {reason:?}");
        if let Some(feeder) = self.feeder.take() {
            feeder.stop();
        }
        if let Some(stats) = self.video.take().and_then(VideoThread::stop)
            && self.play.is_some()
        {
            log::info!("frame stats:\n{}", stats.summary());
        }
        let closed = StreamToLauncher::Closed { reason };
        if let Err(error) = send(&mut io::stdout().lock(), &closed) {
            log::debug!("launcher did not get Closed: {error}");
        }
        event_loop.exit();
    }

    fn close_on_error(&mut self, event_loop: &ActiveEventLoop, result: Result<(), String>) {
        if let Err(message) = result {
            log::error!("stream window error: {message}");
            self.close(event_loop, CloseReason::Error { message });
        }
    }
}

impl ApplicationHandler<StreamEvent> for StreamApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            let result = self.open_window(event_loop);
            self.close_on_error(event_loop, result);
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
            StreamEvent::Stop | StreamEvent::LauncherGone => {
                self.close(event_loop, CloseReason::Stopped);
            }
            StreamEvent::Video(video) => self.on_video(event_loop, video),
        }
    }
}
