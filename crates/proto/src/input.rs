//! Input channel packet v1 (spec 0021), client -> host, one packet each, fixed little-endian layout.
//! The state itself is `mixlink_core::input` (re-exported here); this module is its wire form.

pub use mixlink_core::input::{
    BUTTON_CODE, InputEvent, InputPacket, KEY_BYTES, MAX_EVENTS, MouseMode,
};

use crate::error::InputError;

const KIND_STATE: u8 = 1;
/// kind, seq, mode, buttons, abs x / y, rel x / y, wheel, hwheel, first_event, count, events, keys.
pub const INPUT_LEN: usize = 80;

const EVENTS_AT: usize = 32;
const KEYS_AT: usize = EVENTS_AT + 2 * MAX_EVENTS;

// HOT PATH
pub fn encode(p: &InputPacket, out: &mut [u8; INPUT_LEN]) {
    out[0] = KIND_STATE;
    out[1..5].copy_from_slice(&p.seq.to_le_bytes());
    out[5] = p.mode as u8;
    out[6] = p.buttons;
    out[7..9].copy_from_slice(&p.abs_x.to_le_bytes());
    out[9..11].copy_from_slice(&p.abs_y.to_le_bytes());
    out[11..15].copy_from_slice(&p.rel_x.to_le_bytes());
    out[15..19].copy_from_slice(&p.rel_y.to_le_bytes());
    out[19..23].copy_from_slice(&p.wheel.to_le_bytes());
    out[23..27].copy_from_slice(&p.hwheel.to_le_bytes());
    out[27..31].copy_from_slice(&p.first_event.to_le_bytes());
    let count = usize::from(p.count).min(MAX_EVENTS);
    out[31] = count as u8;
    for (i, slot) in out[EVENTS_AT..KEYS_AT]
        .as_chunks_mut::<2>()
        .0
        .iter_mut()
        .enumerate()
    {
        let event = if i < count {
            p.events[i]
        } else {
            InputEvent::default()
        };
        slot[0] = event.code;
        slot[1] = u8::from(event.down);
    }
    out[KEYS_AT..INPUT_LEN].copy_from_slice(&p.keys);
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

#[cfg(test)]
#[path = "input_tests.rs"]
mod tests;
