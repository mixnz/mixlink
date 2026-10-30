//! Absolute or relative mouse (spec 0021) from the host cursor. A game that takes the mouse hides the
//! cursor for good; Windows also hides the hardware cursor for a moment while a window is dragged or
//! resized (it draws it in the picture instead). So relative needs the cursor hidden for `HIDE_HOLD`
//! with no button held here, and stays until the cursor shows again.

use std::time::{Duration, Instant};

use mixlink_core::input::MouseMode;

/// How long the host cursor must stay hidden (no button held) before relative mode.
pub const HIDE_HOLD: Duration = Duration::from_millis(150);

#[derive(Debug, Clone, Copy)]
pub struct ModePicker {
    visible: bool,
    /// When the current hold started: the cursor hid, or the last button came up while hidden.
    hidden_since: Option<Instant>,
    /// Buttons held on this PC, bit per button.
    buttons: u8,
    relative: bool,
}

impl Default for ModePicker {
    fn default() -> Self {
        Self {
            visible: true,
            hidden_since: None,
            buttons: 0,
            relative: false,
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
        if !self.relative
            && self.buttons == 0
            && self.hidden_since.is_some_and(|at| now >= at + HIDE_HOLD)
        {
            self.relative = true;
        }
        self.mode()
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
        if self.relative || self.buttons != 0 {
            return None;
        }
        self.hidden_since.map(|at| at + HIDE_HOLD)
    }
}

#[cfg(test)]
#[path = "mode_tests.rs"]
mod tests;
