//! The stream window's keyboard and mouse (spec 0021). Captured = focused and not released by the
//! shortcut: the `stream-input` thread hooks the keyboard, and the mouse goes to the host in one of two
//! modes picked from the host cursor: absolute while it shows (desktop; drawn at the local pointer) or
//! relative while a game hides it (pointer confined and hidden, raw motion).

use mixlink_core::input::MouseMode;
use std::time::Instant;

use mixlink_stream::input::capture::to_stream;
use mixlink_stream::input::mode::ModePicker;
use mixlink_stream::input::sender::{InputTx, SendCounts};
use mixlink_stream::input::shortcuts::Shortcut;
use mixlink_stream::input::thread::{InputThread, MouseEvent, MouseFeed};
use mixlink_stream::net::cursor_state::LocalPointer;
use winit::dpi::PhysicalPosition;
use winit::event::{DeviceEvent, ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::EventLoopProxy;
use winit::keyboard::{KeyCode, ModifiersState, PhysicalKey};
use winit::window::{CursorGrabMode, Window};

use crate::ipc::StreamEvent;
use crate::strings;

/// One wheel notch in the units the host gets.
const NOTCH: f64 = 120.0;

#[derive(Debug)]
pub struct WindowInput {
    thread: Option<InputThread>,
    feed: Option<MouseFeed>,
    local: Option<LocalPointer>,
    stream: (u32, u32),
    focused: bool,
    /// Released with the shortcut; a click in the window or the shortcut again takes input back.
    released: bool,
    captured: bool,
    picker: ModePicker,
    modifiers: ModifiersState,
    pointer: Option<PhysicalPosition<f64>>,
    /// Last (captured, mode) written to the log, so only changes are logged.
    logged: Option<(bool, MouseMode)>,
}

impl WindowInput {
    pub fn new() -> Self {
        Self {
            thread: None,
            feed: None,
            local: None,
            stream: (0, 0),
            focused: false,
            released: false,
            captured: false,
            picker: ModePicker::default(),
            modifiers: ModifiersState::empty(),
            pointer: None,
            logged: None,
        }
    }

    /// Starts `stream-input` once the session is up; shortcuts come back as `StreamEvent::Shortcut`.
    pub fn start(
        &mut self,
        window: &Window,
        tx: InputTx,
        stream: (u32, u32),
        local: LocalPointer,
        proxy: EventLoopProxy<StreamEvent>,
    ) {
        let started = InputThread::start(tx, move |shortcut| {
            // Fails only after the loop has exited; nothing left to do.
            let _ = proxy.send_event(StreamEvent::Shortcut(shortcut));
        });
        match started {
            Ok((thread, feed)) => {
                self.thread = Some(thread);
                self.feed = Some(feed);
                self.local = Some(local);
                self.stream = stream;
                self.focused = window.has_focus();
                self.update(window);
            }
            Err(error) => log::warn!("input thread not started, keys and mouse stay here: {error}"),
        }
    }

    /// Sends the all-released state and stops the thread.
    pub fn stop(&mut self) -> Option<SendCounts> {
        self.feed = None;
        self.thread.take().map(InputThread::stop)
    }

    pub fn host_cursor(&mut self, window: &Window, visible: bool) {
        log::info!("host cursor {}", if visible { "shown" } else { "hidden" });
        self.picker.host_cursor(visible, Instant::now());
        self.poll(window, Instant::now());
    }

    /// Switches the mouse mode when the picker says so; returns when to look again (spec 0021: the
    /// host cursor must stay hidden for a while before relative mode).
    pub fn poll(&mut self, window: &Window, now: Instant) -> Option<Instant> {
        let before = self.picker.mode();
        if self.picker.update(now) != before {
            self.captured_mode_changed(window);
        }
        self.picker.next_check()
    }

    /// The release shortcut (typed while captured).
    pub fn release(&mut self, window: &Window) {
        self.released = true;
        self.update(window);
    }

    /// Window events; returns a shortcut typed while input is not captured (the hook has them otherwise).
    pub fn window_event(&mut self, window: &Window, event: &WindowEvent) -> Option<Shortcut> {
        match event {
            WindowEvent::Focused(focused) => {
                self.focused = *focused;
                self.update(window);
            }
            WindowEvent::ModifiersChanged(modifiers) => self.modifiers = modifiers.state(),
            WindowEvent::KeyboardInput { event, .. }
                if event.state == ElementState::Pressed && !event.repeat =>
            {
                return self.local_shortcut(window, event.physical_key);
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.pointer = Some(*position);
                self.absolute_move(window);
            }
            WindowEvent::MouseInput { state, button, .. } => {
                self.button(window, *state, *button);
            }
            WindowEvent::MouseWheel { delta, .. } if self.captured => {
                let (v, h) = match *delta {
                    MouseScrollDelta::LineDelta(x, y) => {
                        (f64::from(y) * NOTCH, f64::from(x) * NOTCH)
                    }
                    MouseScrollDelta::PixelDelta(p) => (p.y, p.x),
                };
                self.push(MouseEvent::Wheel(v.round() as i32, h.round() as i32));
            }
            _ => {}
        }
        None
    }

    /// Raw motion: sent only in relative mode.
    // HOT PATH
    pub fn device_event(&mut self, event: &DeviceEvent) {
        if let DeviceEvent::MouseMotion { delta } = event
            && self.captured
            && self.mode() == MouseMode::Relative
        {
            self.push(MouseEvent::Rel(
                delta.0.round() as i32,
                delta.1.round() as i32,
            ));
        }
    }

    fn mode(&self) -> MouseMode {
        self.picker.mode()
    }

    fn local_shortcut(&mut self, window: &Window, key: PhysicalKey) -> Option<Shortcut> {
        let m = self.modifiers;
        if !(m.control_key() && m.alt_key() && m.shift_key()) {
            return None;
        }
        match key {
            PhysicalKey::Code(KeyCode::KeyZ) => {
                self.released = !self.released;
                self.update(window);
                None
            }
            PhysicalKey::Code(KeyCode::KeyF) => Some(Shortcut::Fullscreen),
            PhysicalKey::Code(KeyCode::KeyQ) => Some(Shortcut::Close),
            _ => None,
        }
    }

    fn button(&mut self, window: &Window, state: ElementState, button: MouseButton) {
        let down = state == ElementState::Pressed;
        if !self.captured {
            // A click on a released window takes input back; the click itself stays here.
            if down && self.released && self.focused && self.thread.is_some() {
                self.released = false;
                self.update(window);
            }
            return;
        }
        let n = match button {
            MouseButton::Left => 0,
            MouseButton::Right => 1,
            MouseButton::Middle => 2,
            MouseButton::Back => 3,
            MouseButton::Forward => 4,
            MouseButton::Other(_) => return,
        };
        self.picker.button(n, down, Instant::now());
        self.push(MouseEvent::Button(n, down));
    }

    // HOT PATH
    fn absolute_move(&mut self, window: &Window) {
        if !self.captured || self.mode() != MouseMode::Absolute {
            return;
        }
        let Some(pos) = self.pointer else {
            return;
        };
        let size = window.inner_size();
        if let Some((x, y)) = to_stream((pos.x, pos.y), (size.width, size.height), self.stream) {
            self.push(MouseEvent::Abs(x, y));
            if let Some(local) = &self.local {
                local.set(x, y);
            }
        }
    }

    /// Takes or gives back input as focus and the release shortcut say.
    fn update(&mut self, window: &Window) {
        let want = self.focused && !self.released && self.thread.is_some();
        if want != self.captured {
            self.captured = want;
            self.push(MouseEvent::Capture(want));
            window.set_title(if want {
                strings::TITLE_CAPTURED
            } else {
                strings::WINDOW_TITLE
            });
        }
        self.captured_mode_changed(window);
    }

    /// Pointer grab, local cursor and the mode the host uses.
    fn captured_mode_changed(&mut self, window: &Window) {
        let mode = self.mode();
        if self.captured {
            self.push(MouseEvent::Mode(mode));
        }
        if self.logged != Some((self.captured, mode)) {
            self.logged = Some((self.captured, mode));
            log::info!("input captured: {}, mouse mode: {mode:?}", self.captured);
        }
        let grab = match (self.captured, mode) {
            (true, MouseMode::Relative) => CursorGrabMode::Confined,
            _ => CursorGrabMode::None,
        };
        if let Err(error) = window.set_cursor_grab(grab) {
            log::debug!("pointer grab {grab:?} refused: {error}");
        }
        window.set_cursor_visible(!self.captured);
        let local_drawn = self.captured && mode == MouseMode::Absolute;
        if !local_drawn && let Some(local) = &self.local {
            local.clear();
        }
        self.absolute_move(window);
    }

    fn push(&mut self, event: MouseEvent) {
        if let Some(feed) = self.feed.as_mut()
            && !feed.push(event)
        {
            log::trace!("input event dropped: {event:?}");
        }
    }
}
