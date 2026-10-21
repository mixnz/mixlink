//! Window lifecycle: one window, cleared to a solid color, reporting to the launcher.

use std::io;
use std::sync::Arc;

use mixlink_core::ipc::{CloseReason, IPC_VERSION, StreamToLauncher};
use mixlink_render::Presenter;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowId};

use crate::ipc::{StreamEvent, send};
use crate::strings;

const CLEAR_COLOR: [f32; 4] = [0.05, 0.25, 0.30, 1.0];

#[derive(Debug, Default)]
pub struct StreamApp {
    window: Option<Arc<Window>>,
    presenter: Option<Presenter>,
    ready_sent: bool,
    closing: bool,
}

impl StreamApp {
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
        let presenter = Presenter::new(Arc::clone(&window), size.width, size.height)
            .map_err(|e| e.to_string())?;
        window.request_redraw();
        self.presenter = Some(presenter);
        self.window = Some(window);
        Ok(())
    }

    fn redraw(&mut self) -> Result<(), String> {
        let Some(presenter) = self.presenter.as_mut() else {
            return Ok(());
        };
        presenter
            .clear_and_present(CLEAR_COLOR)
            .map_err(|e| e.to_string())?;
        if !self.ready_sent {
            self.ready_sent = true;
            let ready = StreamToLauncher::Ready {
                ipc_version: IPC_VERSION,
            };
            send(&mut io::stdout().lock(), &ready).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    fn resize(&mut self, width: u32, height: u32) -> Result<(), String> {
        if let Some(presenter) = self.presenter.as_mut() {
            presenter.resize(width, height).map_err(|e| e.to_string())?;
        }
        if let Some(window) = &self.window {
            window.request_redraw();
        }
        Ok(())
    }

    /// Tells the launcher why we close (it may already be gone), then stops the loop.
    fn close(&mut self, event_loop: &ActiveEventLoop, reason: CloseReason) {
        if self.closing {
            return;
        }
        self.closing = true;
        log::info!("closing: {reason:?}");
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
                let result = self.resize(size.width, size.height);
                self.close_on_error(event_loop, result);
            }
            WindowEvent::RedrawRequested => {
                let result = self.redraw();
                self.close_on_error(event_loop, result);
            }
            _ => {}
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: StreamEvent) {
        match event {
            StreamEvent::Stop | StreamEvent::LauncherGone => {
                self.close(event_loop, CloseReason::Stopped);
            }
        }
    }
}
