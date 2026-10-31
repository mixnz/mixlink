//! Absolute or relative mouse (spec 0021) from the host cursor. A game that takes the mouse hides the
//! cursor for good; Windows also hides the hardware cursor for a moment while a window is dragged or
//! resized (it draws it in the picture instead). So relative needs the cursor hidden for `HIDE_HOLD`
//! with no button held here and no key typed for `TYPING_QUIET`, and stays until the cursor shows again.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use mixlink_core::input::MouseMode;

/// How long the host cursor must stay hidden (no button held) before relative mode.
pub const HIDE_HOLD: Duration = Duration::from_millis(150);
/// Windows hides the cursor while typing ("hide pointer while typing"): no relative mode until the keys
/// have been quiet this long.
pub const TYPING_QUIET: Duration = Duration::from_secs(1);

/// When a key last changed, shared by `stream-input` (writes) and the window (reads).
#[derive(Debug, Clone)]
pub struct KeyActivity {
    base: Instant,
    /// Microseconds after `base`, plus one; 0 = no key yet.
    last_us: Arc<AtomicU64>,
}

impl Default for KeyActivity {
    fn default() -> Self {
        Self::new()
    }
}

impl KeyActivity {
    pub fn new() -> Self {
        Self {
            base: Instant::now(),
            last_us: Arc::new(AtomicU64::new(0)),
        }
    }

    // HOT PATH
    pub fn note(&self, now: Instant) {
        let us = u64::try_from(now.saturating_duration_since(self.base).as_micros())
            .unwrap_or(u64::MAX - 1);
        self.last_us.store(us + 1, Ordering::Relaxed);
    }

    pub fn last(&self) -> Option<Instant> {
        match self.last_us.load(Ordering::Relaxed) {
            0 => None,
            us => Some(self.base + Duration::from_micros(us - 1)),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ModePicker {
    visible: bool,
    /// When the current hold started: the cursor hid, or the last button came up while hidden.
    hidden_since: Option<Instant>,
    /// Buttons held on this PC, bit per button.
    buttons: u8,
    relative: bool,
    /// The mode the window and the host were last switched to (`take_change`).
    applied: MouseMode,
}

impl Default for ModePicker {
    fn default() -> Self {
        Self {
            visible: true,
            hidden_since: None,
            buttons: 0,
            relative: false,
            applied: MouseMode::Absolute,
        }
    }
}

impl ModePicker {
    pub fn host_cursor(&mut self, visible: bool, now: Instant) {
        if visible == self.visible {
            return;
        }
        self.visible = visible;
        if visible {
            self.relative = false;
            self.hidden_since = None;
        } else {
            self.hidden_since = Some(now);
        }
    }

    pub fn button(&mut self, n: u8, down: bool, now: Instant) {
        let bit = 1u8 << n.min(7);
        if down {
            self.buttons |= bit;
        } else {
            self.buttons &= !bit;
            if self.buttons == 0 && !self.visible {
                self.hidden_since = Some(now);
            }
        }
    }

    /// The mode at `now` (moves to relative once the hold is over).
    pub fn update(&mut self, now: Instant) -> MouseMode {
        self.update_after(now, None)
    }

    /// As `update`, with the last key change: relative waits for `TYPING_QUIET` after it.
    pub fn update_after(&mut self, now: Instant, last_key: Option<Instant>) -> MouseMode {
        if !self.relative && self.next_check_after(last_key).is_some_and(|at| now >= at) {
            self.relative = true;
        }
        self.mode()
    }

    /// The mode at `now` when it differs from the one applied (which it then becomes); the window
    /// switches grab, cursor and host mode on `Some`.
    pub fn take_change(&mut self, now: Instant, last_key: Option<Instant>) -> Option<MouseMode> {
        let mode = self.update_after(now, last_key);
        if mode == self.applied {
            return None;
        }
        self.applied = mode;
        Some(mode)
    }

    /// The mode the window and the host use now.
    pub fn applied(&self) -> MouseMode {
        self.applied
    }

    pub fn mode(&self) -> MouseMode {
        if self.relative {
            MouseMode::Relative
        } else {
            MouseMode::Absolute
        }
    }

    /// When `update` may switch to relative, if it can.
    pub fn next_check(&self) -> Option<Instant> {
        self.next_check_after(None)
    }

    pub fn next_check_after(&self, last_key: Option<Instant>) -> Option<Instant> {
        if self.relative || self.buttons != 0 {
            return None;
        }
        let held = self.hidden_since? + HIDE_HOLD;
        Some(last_key.map_or(held, |key| held.max(key + TYPING_QUIET)))
    }
}

#[cfg(test)]
#[path = "mode_tests.rs"]
mod tests;
