//! Input channel packet v1 (spec 0021), client -> host, one packet each, fixed little-endian layout.
//! Each packet carries the whole input state, so a lost datagram is made good by the next one: running
//! mouse totals, the last `MAX_EVENTS` key / button events in order, and the bitmap of keys held.

use crate::error::InputError;

const KIND_STATE: u8 = 1;
/// kind, seq, mode, buttons, abs x / y, rel x / y, wheel, hwheel, first_event, count, events, keys.
pub const INPUT_LEN: usize = 80;
/// Key / button events a packet repeats.
pub const MAX_EVENTS: usize = 8;
/// Event code of mouse button `n` (0..=4) is `BUTTON_CODE + n`; key codes are HID usages (page 0x07).
pub const BUTTON_CODE: u8 = 0xF0;
/// Bitmap of HID usages 0..=255.
pub const KEY_BYTES: usize = 32;

const EVENTS_AT: usize = 32;
const KEYS_AT: usize = EVENTS_AT + 2 * MAX_EVENTS;

/// How the host moves its pointer.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum MouseMode {
    #[default]
    None = 0,
    /// `abs_x` / `abs_y` in stream pixels.
    Absolute = 1,
    /// `rel_x` / `rel_y` running totals.
    Relative = 2,
}

impl MouseMode {
    const fn from_byte(byte: u8) -> Option<MouseMode> {
        match byte {
            0 => Some(MouseMode::None),
            1 => Some(MouseMode::Absolute),
            2 => Some(MouseMode::Relative),
            _ => None,
        }
    }
}

/// A key (HID usage) or button (`BUTTON_CODE + n`) going down or up.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct InputEvent {
    pub code: u8,
    pub down: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputPacket {
    /// Packet number; repeats of the same state keep it.
    pub seq: u32,
    pub mode: MouseMode,
    /// Bit n = button n held, after every event below.
    pub buttons: u8,
    pub abs_x: u16,
    pub abs_y: u16,
    /// Running totals since the session started (wrapping).
    pub rel_x: i32,
    pub rel_y: i32,
    /// Running totals in 1/120 notch.
    pub wheel: i32,
    pub hwheel: i32,
    /// Number of `events[0]`; events are numbered from 1.
    pub first_event: u32,
    /// Events used in `events`.
    pub count: u8,
    pub events: [InputEvent; MAX_EVENTS],
    /// Bit `usage` set = key held after the events.
    pub keys: [u8; KEY_BYTES],
}

impl InputPacket {
    // HOT PATH
    pub fn encode(&self, out: &mut [u8; INPUT_LEN]) {
        out[0] = KIND_STATE;
        out[1..5].copy_from_slice(&self.seq.to_le_bytes());
        out[5] = self.mode as u8;
        out[6] = self.buttons;
        out[7..9].copy_from_slice(&self.abs_x.to_le_bytes());
        out[9..11].copy_from_slice(&self.abs_y.to_le_bytes());
        out[11..15].copy_from_slice(&self.rel_x.to_le_bytes());
        out[15..19].copy_from_slice(&self.rel_y.to_le_bytes());
        out[19..23].copy_from_slice(&self.wheel.to_le_bytes());
        out[23..27].copy_from_slice(&self.hwheel.to_le_bytes());
        out[27..31].copy_from_slice(&self.first_event.to_le_bytes());
        let count = usize::from(self.count).min(MAX_EVENTS);
        out[31] = count as u8;
        for (i, slot) in out[EVENTS_AT..KEYS_AT].chunks_exact_mut(2).enumerate() {
            let event = if i < count {
                self.events[i]
            } else {
                InputEvent::default()
            };
            slot[0] = event.code;
            slot[1] = u8::from(event.down);
        }
        out[KEYS_AT..INPUT_LEN].copy_from_slice(&self.keys);
    }

    // HOT PATH
    pub fn decode(bytes: &[u8]) -> Result<InputPacket, InputError> {
        let Some(b) = bytes.first_chunk::<INPUT_LEN>() else {
            return Err(InputError::TooShort { len: bytes.len() });
        };
        if b[0] != KIND_STATE {
            return Err(InputError::Kind { byte: b[0] });
        }
        let mode = MouseMode::from_byte(b[5]).ok_or(InputError::Mode { byte: b[5] })?;
        let count = b[31];
        if usize::from(count) > MAX_EVENTS {
            return Err(InputError::Count { count });
        }
        let u32_at = |at: usize| u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]]);
        let i32_at = |at: usize| i32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]]);
        let mut events = [InputEvent::default(); MAX_EVENTS];
        for (i, event) in events.iter_mut().take(usize::from(count)).enumerate() {
            let at = EVENTS_AT + 2 * i;
            *event = InputEvent {
                code: b[at],
                down: b[at + 1] != 0,
            };
        }
        let mut keys = [0u8; KEY_BYTES];
        keys.copy_from_slice(&b[KEYS_AT..INPUT_LEN]);
        Ok(InputPacket {
            seq: u32_at(1),
            mode,
            buttons: b[6],
            abs_x: u16::from_le_bytes([b[7], b[8]]),
            abs_y: u16::from_le_bytes([b[9], b[10]]),
            rel_x: i32_at(11),
            rel_y: i32_at(15),
            wheel: i32_at(19),
            hwheel: i32_at(23),
            first_event: u32_at(27),
            count,
            events,
            keys,
        })
    }
}

#[cfg(test)]
#[path = "input_tests.rs"]
mod tests;
