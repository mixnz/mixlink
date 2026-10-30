//! Thread `stream-input` (spec 0021): owns the input state and the sealed sender. While input is
//! captured, keys come from the keyboard hook thread (`hook_thread`, its own `GetMessageW` loop, so a
//! busy or modal winit thread never delays them) over an SPSC ring plus a wake; mouse events come
//! from the winit thread the same way.

use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use mixlink_core::input::MouseMode;
use mixlink_core::mmcss::{Mmcss, MmcssTask};
use mixlink_input::client::state::InputState;
use mixlink_input::keymap;
use mixlink_input::platform::windows::hook_thread::HookThread;
use mixlink_input::platform::windows::kbd_hook::RawKey;
use mixlink_input::platform::windows::thread_msg::{Pump, Pumped, Waker};
use rtrb::{Consumer, Producer, RingBuffer};

use super::sender::{InputTx, Schedule, SendCounts};
use super::shortcuts::{KeyUse, Shortcut, Shortcuts};

/// Mouse events in flight from the winit thread (1 kHz mice, a slow wake: still far from full).
const RING: usize = 1024;
/// Keys in flight from the hook thread.
const KEY_RING: usize = 256;
/// How often the loop logs its numbers while the hook is on (diagnostics).
const HEARTBEAT: Duration = Duration::from_secs(2);

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

/// The hook while input is captured: its thread, the keys it hands over, and how many it saw.
struct Hooked {
    thread: HookThread,
    keys: Consumer<RawKey>,
    seen: Arc<AtomicU64>,
}

/// The loop since the last heartbeat.
#[derive(Debug, Default)]
struct LoopStats {
    woken: u64,
    timeouts: u64,
    longest_tick: Duration,
}

struct Worker {
    state: InputState,
    tx: InputTx,
    schedule: Schedule,
    shortcuts: Shortcuts,
    ring: Consumer<MouseEvent>,
    captured: bool,
    hook: Option<Hooked>,
    on_shortcut: OnShortcut,
    waker: Waker,
    keys_sent: u64,
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
    let mut worker = Worker::new(tx, ring, on_shortcut, pump.waker());
    let _ = ready.send(Ok(pump.waker()));
    let mut stats = LoopStats::default();
    let mut beat = Instant::now();
    loop {
        let started = Instant::now();
        let deadline = worker.tick(started);
        stats.longest_tick = stats.longest_tick.max(started.elapsed());
        // Diagnostics (spec 0021 run): wake at least every second, for the heartbeat.
        let cap = started + HEARTBEAT / 2;
        match pump.wait(Some(deadline.map_or(cap, |at| at.min(cap)))) {
            Pumped::Quit => break,
            Pumped::Woken => stats.woken += 1,
            Pumped::Timeout => stats.timeouts += 1,
        }
        if beat.elapsed() >= HEARTBEAT {
            worker.heartbeat(&stats);
            stats = LoopStats::default();
            beat = Instant::now();
        }
    }
    worker.finish()
}

impl Worker {
    fn new(tx: InputTx, ring: Consumer<MouseEvent>, on_shortcut: OnShortcut, waker: Waker) -> Self {
        Self {
            state: InputState::new(),
            tx,
            schedule: Schedule::default(),
            shortcuts: Shortcuts::default(),
            ring,
            captured: false,
            hook: None,
            on_shortcut,
            waker,
            keys_sent: 0,
        }
    }

    /// Takes the hook's keys and the winit thread's events, sends a new state or a due repeat;
    /// returns the next wake time.
    // HOT PATH
    fn tick(&mut self, now: Instant) -> Option<Instant> {
        let mut changed = false;
        while let Some(key) = self.hook.as_mut().and_then(|h| h.keys.pop().ok()) {
            changed |= self.key(key);
        }
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
            self.hook = self.start_hook();
            return false;
        }
        self.unhook();
        self.shortcuts.reset();
        self.state.release_all()
    }

    /// The hook thread swallows every key with a HID usage and hands it over; a full ring lets the
    /// key through here rather than lose it.
    fn start_hook(&self) -> Option<Hooked> {
        let (mut keys_tx, keys) = RingBuffer::new(KEY_RING);
        let seen = Arc::new(AtomicU64::new(0));
        let counter = Arc::clone(&seen);
        let waker = self.waker.clone();
        let sink = Box::new(move |key: RawKey| {
            // HOT PATH (hook thread)
            counter.fetch_add(1, Ordering::Relaxed);
            if keymap::usage(key.scan, key.extended, key.vk).is_none() {
                return false;
            }
            let taken = keys_tx.push(key).is_ok();
            waker.wake();
            taken
        });
        match HookThread::start(sink) {
            Ok(thread) => {
                log::info!("keyboard hook on");
                Some(Hooked { thread, keys, seen })
            }
            Err(error) => {
                log::warn!("keyboard hook not installed, keys stay on this PC: {error}");
                None
            }
        }
    }

    /// One key from the hook (already swallowed there).
    // HOT PATH
    fn key(&mut self, key: RawKey) -> bool {
        let Some(usage) = keymap::usage(key.scan, key.extended, key.vk) else {
            return false;
        };
        match self.shortcuts.key(usage, key.down) {
            KeyUse::Forward => {
                let changed = self.state.key(usage, key.down);
                self.keys_sent += u64::from(changed);
                changed
            }
            KeyUse::Drop => false,
            KeyUse::Run(shortcut) => {
                let released = shortcut != Shortcut::Fullscreen && self.state.release_all();
                (self.on_shortcut)(shortcut);
                released
            }
        }
    }

    fn send_new(&mut self, now: Instant) {
        let packet = self.state.packet();
        self.tx.send(&packet, false);
        self.schedule.on_send(now);
    }

    /// Diagnostics: is the thread alive and is the hook being called?
    fn heartbeat(&self, stats: &LoopStats) {
        if let Some(hook) = &self.hook {
            log::info!(
                "stream-input: {} wakes, {} timer ends, longest tick {} us; hook calls so far {}",
                stats.woken,
                stats.timeouts,
                stats.longest_tick.as_micros(),
                hook.seen.load(Ordering::Relaxed)
            );
        }
    }

    /// Removes the hook and logs what it saw.
    fn unhook(&mut self) {
        if let Some(hook) = self.hook.take() {
            let seen = hook.thread.stop();
            log::info!(
                "keyboard hook off: {} keys seen, {} injected (passed on), {} swallowed, {} key changes sent",
                seen.calls,
                seen.injected,
                seen.swallowed,
                self.keys_sent
            );
            self.keys_sent = 0;
        }
    }

    fn finish(&mut self) -> SendCounts {
        self.unhook();
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
