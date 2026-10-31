//! The client's input state (spec 0021): what is held, running mouse totals, and the last key / button
//! events, turned into one `InputPacket` per change. No heap.

use mixlink_core::input::{BUTTON_CODE, InputEvent, InputPacket, KEY_BYTES, MAX_EVENTS, MouseMode};

/// Mouse buttons 0..=4 (left, right, middle, back, forward).
const BUTTONS: u8 = 5;

#[derive(Debug, Clone)]
pub struct InputState {
    seq: u32,
    mode: MouseMode,
    buttons: u8,
    abs: (u16, u16),
    rel: (i32, i32),
    wheel: (i32, i32),
    keys: [u8; KEY_BYTES],
    /// Event number n lives at `ring[n % MAX_EVENTS]`.
    ring: [InputEvent; MAX_EVENTS],
    /// Number the next event gets; events are numbered from 1.
    next_event: u32,
}

impl Default for InputState {
    fn default() -> Self {
        Self::new()
    }
}

impl InputState {
    pub const fn new() -> Self {
        Self {
            seq: 0,
            mode: MouseMode::None,
            buttons: 0,
            abs: (0, 0),
            rel: (0, 0),
            wheel: (0, 0),
            keys: [0; KEY_BYTES],
            ring: [InputEvent {
                code: 0,
                down: false,
            }; MAX_EVENTS],
            next_event: 1,
        }
    }

    /// Key with HID usage `usage` down / up. `false`: no change (a key repeat), nothing to send.
    // HOT PATH
    pub fn key(&mut self, usage: u8, down: bool) -> bool {
        let (byte, bit) = (usize::from(usage / 8), 1u8 << (usage % 8));
        if (self.keys[byte] & bit != 0) == down {
            return false;
        }
        self.keys[byte] ^= bit;
        self.push(usage, down);
        true
    }

    /// Mouse button `n` (0..=4) down / up. `false`: no change or no such button.
    // HOT PATH
    pub fn button(&mut self, n: u8, down: bool) -> bool {
        if n >= BUTTONS {
            return false;
        }
        let bit = 1u8 << n;
        if (self.buttons & bit != 0) == down {
            return false;
        }
        self.buttons ^= bit;
        self.push(BUTTON_CODE + n, down);
        true
    }

    /// Pointer at `x`, `y` in stream pixels.
    pub fn mouse_abs(&mut self, x: u16, y: u16) {
        self.abs = (x, y);
    }

    /// Raw motion; totals wrap.
    pub fn mouse_rel(&mut self, dx: i32, dy: i32) {
        self.rel = (self.rel.0.wrapping_add(dx), self.rel.1.wrapping_add(dy));
    }

    /// Wheel in 1/120 notch; totals wrap.
    pub fn wheel(&mut self, v: i32, h: i32) {
        self.wheel = (self.wheel.0.wrapping_add(v), self.wheel.1.wrapping_add(h));
    }

    pub fn set_mode(&mut self, mode: MouseMode) {
        self.mode = mode;
    }

    /// Up events for every held key and button. `false`: nothing was held.
    pub fn release_all(&mut self) -> bool {
        let mut any = false;
        for usage in 0..=u8::MAX {
            any |= self.key(usage, false);
        }
        for n in 0..BUTTONS {
            any |= self.button(n, false);
        }
        any
    }

    /// Any key or button down (the sender keeps the host alive while so).
    pub fn held(&self) -> bool {
        self.buttons != 0 || self.keys.iter().any(|&b| b != 0)
    }

    /// The state as a new packet (next `seq`).
    // HOT PATH
    pub fn packet(&mut self) -> InputPacket {
        self.seq = self.seq.wrapping_add(1);
        self.current()
    }

    /// The state with the last packet's `seq`, for repeats.
    // HOT PATH
    pub fn current(&self) -> InputPacket {
        let sent = self.next_event - 1;
        let count = sent.min(MAX_EVENTS as u32);
        let first_event = self.next_event - count;
        let mut events = [InputEvent::default(); MAX_EVENTS];
        for (i, event) in events.iter_mut().take(count as usize).enumerate() {
            *event = self.ring[(first_event as usize + i) % MAX_EVENTS];
        }
        InputPacket {
            seq: self.seq,
            mode: self.mode,
            buttons: self.buttons,
            abs_x: self.abs.0,
            abs_y: self.abs.1,
            rel_x: self.rel.0,
            rel_y: self.rel.1,
            wheel: self.wheel.0,
            hwheel: self.wheel.1,
            first_event,
            count: count as u8,
            events,
            keys: self.keys,
        }
    }

    fn push(&mut self, code: u8, down: bool) {
        self.ring[self.next_event as usize % MAX_EVENTS] = InputEvent { code, down };
        self.next_event = self.next_event.wrapping_add(1).max(1);
    }
}

#[cfg(test)]
#[path = "state_tests.rs"]
mod tests;
