//! Thread `stream-input` (spec 0021): owns the input state and the sealed sender. Keys come from a
//! low-level hook installed on this thread while input is captured (so a busy or modal winit thread
//! never delays them); mouse events come from the winit thread over an SPSC ring plus a wake.

use std::cell::RefCell;
use std::io;
use std::sync::mpsc;
use std::thread::{self, JoinHandle};
use std::time::Instant;

use mixlink_core::input::MouseMode;
use mixlink_core::mmcss::{Mmcss, MmcssTask};
use mixlink_input::client::state::InputState;
use mixlink_input::keymap;
use mixlink_input::platform::windows::kbd_hook::{KeyHook, RawKey};
use mixlink_input::platform::windows::thread_msg::{Pump, Pumped, Waker};
use rtrb::{Consumer, Producer, RingBuffer};

use super::sender::{InputTx, Schedule, SendCounts};
use super::shortcuts::{KeyUse, Shortcut, Shortcuts};

/// Mouse events in flight from the winit thread (1 kHz mice, a slow wake: still far from full).
const RING: usize = 1024;

/// What the winit thread tells `stream-input`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseEvent {
    /// Pointer at stream pixels.
    Abs(u16, u16),
    /// Raw motion.
    Rel(i32, i32),
    /// Button 0..=4 down / up.
    Button(u8, bool),
    /// Wheel in 1/120 notch: vertical, horizontal.
    Wheel(i32, i32),
    Mode(MouseMode),
    /// Window focused and not released: keys and mouse go to the host.
    Capture(bool),
}

/// The winit thread's end.
#[derive(Debug)]
pub struct MouseFeed {
    tx: Producer<MouseEvent>,
    waker: Waker,
}

impl MouseFeed {
    /// `false` when the ring is full or the thread is gone.
    // HOT PATH
    pub fn push(&mut self, event: MouseEvent) -> bool {
        self.tx.push(event).is_ok() && self.waker.wake()
    }
}

#[derive(Debug)]
pub struct InputThread {
    waker: Waker,
    handle: JoinHandle<SendCounts>,
}

type OnShortcut = Box<dyn Fn(Shortcut) + Send>;

impl InputThread {
    pub fn start(
        tx: InputTx,
        on_shortcut: impl Fn(Shortcut) + Send + 'static,
    ) -> io::Result<(InputThread, MouseFeed)> {
        let (ring_tx, ring_rx) = RingBuffer::new(RING);
        let (ready, waker_rx) = mpsc::sync_channel(1);
        let on_shortcut: OnShortcut = Box::new(on_shortcut);
        let handle = thread::Builder::new()
            .name("stream-input".into())
            .spawn(move || run(tx, ring_rx, on_shortcut, &ready))?;
        let waker = waker_rx
            .recv()
            .map_err(|_| io::Error::other("input thread did not start"))?
            .map_err(io::Error::other)?;
        let feed = MouseFeed {
            tx: ring_tx,
            waker: waker.clone(),
        };
        Ok((InputThread { waker, handle }, feed))
    }

    /// Sends the all-released state, removes the hook, and waits for the thread.
    pub fn stop(self) -> SendCounts {
        self.waker.quit();
        self.handle.join().unwrap_or_default()
    }
}

struct Worker {
    state: InputState,
    tx: InputTx,
    schedule: Schedule,
    shortcuts: Shortcuts,
    ring: Consumer<MouseEvent>,
    captured: bool,
    hook: Option<KeyHook>,
    on_shortcut: OnShortcut,
    /// This thread's own wake: a key sent from the hook must make the wait take the new deadline.
    waker: Waker,
}

thread_local! {
    // Set for the thread's life; the hook borrows it only inside `Pump::wait`, never while `run` does.
    static WORKER: RefCell<Option<Worker>> = const { RefCell::new(None) };
}

fn run(
    tx: InputTx,
    ring: Consumer<MouseEvent>,
    on_shortcut: OnShortcut,
    ready: &mpsc::SyncSender<Result<Waker, String>>,
) -> SendCounts {
    let pump = match Pump::new() {
        Ok(pump) => pump,
        Err(error) => {
            let _ = ready.send(Err(error.to_string()));
            return SendCounts::default();
        }
    };
    let _mmcss = Mmcss::enter(MmcssTask::Games);
    install_worker(tx, ring, on_shortcut, pump.waker());
    let _ = ready.send(Ok(pump.waker()));
    loop {
        let deadline = with_worker(|w| w.tick(Instant::now())).flatten();
        if pump.wait(deadline) == Pumped::Quit {
            break;
        }
    }
    let counts = with_worker(Worker::finish).unwrap_or_default();
    WORKER.with(|w| w.borrow_mut().take());
    counts
}

fn install_worker(tx: InputTx, ring: Consumer<MouseEvent>, on_shortcut: OnShortcut, waker: Waker) {
    WORKER.with(|w| {
        *w.borrow_mut() = Some(Worker {
            state: InputState::new(),
            tx,
            schedule: Schedule::default(),
            shortcuts: Shortcuts::default(),
            ring,
            captured: false,
            hook: None,
            on_shortcut,
            waker,
        });
    });
}

fn with_worker<T>(f: impl FnOnce(&mut Worker) -> T) -> Option<T> {
    WORKER.with(|w| w.try_borrow_mut().ok()?.as_mut().map(f))
}

/// The hook: `true` swallows the key (it went to the host, or was a shortcut).
// HOT PATH
fn on_key(key: RawKey) -> bool {
    with_worker(|w| w.key(key)).unwrap_or(false)
}

impl Worker {
    /// Takes the winit thread's events, sends a new state or a due repeat; returns the next wake time.
    // HOT PATH
    fn tick(&mut self, now: Instant) -> Option<Instant> {
        let mut changed = false;
        while let Ok(event) = self.ring.pop() {
            changed |= self.mouse(event);
        }
        let held = self.state.held();
        if changed {
            self.send_new(now);
        } else if self.schedule.fire(now, held) {
            self.tx.send(&self.state.current(), true);
        }
        self.schedule.next(self.state.held())
    }

    fn mouse(&mut self, event: MouseEvent) -> bool {
        if let MouseEvent::Capture(on) = event {
            return self.capture(on);
        }
        if !self.captured {
            return false;
        }
        match event {
            MouseEvent::Abs(x, y) => self.state.mouse_abs(x, y),
            MouseEvent::Rel(dx, dy) => self.state.mouse_rel(dx, dy),
            MouseEvent::Wheel(v, h) => self.state.wheel(v, h),
            MouseEvent::Mode(mode) => self.state.set_mode(mode),
            MouseEvent::Button(n, down) => return self.state.button(n, down),
            MouseEvent::Capture(_) => {}
        }
        true
    }

    /// Hook on / off. Off releases everything on the host first.
    fn capture(&mut self, on: bool) -> bool {
        if on == self.captured {
            return false;
        }
        self.captured = on;
        if on {
            match KeyHook::install(on_key) {
                Ok(hook) => self.hook = Some(hook),
                Err(error) => {
                    log::warn!("keyboard hook not installed, keys stay on this PC: {error}")
                }
            }
            return false;
        }
        self.hook = None;
        self.shortcuts.reset();
        self.state.release_all()
    }

    // HOT PATH
    fn key(&mut self, key: RawKey) -> bool {
        let Some(usage) = keymap::usage(key.scan, key.extended, key.vk) else {
            return false;
        };
        match self.shortcuts.key(usage, key.down) {
            KeyUse::Forward => {
                if self.state.key(usage, key.down) {
                    self.send_new(Instant::now());
                    // The wait runs with the deadline from before this key; take the new one.
                    self.waker.wake();
                }
            }
            KeyUse::Drop => {}
            KeyUse::Run(shortcut) => {
                if shortcut != Shortcut::Fullscreen && self.state.release_all() {
                    self.send_new(Instant::now());
                    self.waker.wake();
                }
                (self.on_shortcut)(shortcut);
            }
        }
        true
    }

    fn send_new(&mut self, now: Instant) {
        let packet = self.state.packet();
        self.tx.send(&packet, false);
        self.schedule.on_send(now);
    }

    fn finish(&mut self) -> SendCounts {
        self.hook = None;
        self.captured = false;
        self.state.release_all();
        // Sent even when nothing was held: the host then knows this state is final.
        let packet = self.state.packet();
        self.tx.send(&packet, false);
        self.tx.send(&packet, true);
        self.tx.counts()
    }
}

#[cfg(test)]
#[path = "thread_tests.rs"]
mod tests;
