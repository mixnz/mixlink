//! Client input state as it crosses to the host (spec 0021): shared by `mixlink-input` (builds and
//! applies it) and `mixlink-proto` (wire layout, `proto::input`).

/// Key / button events a packet repeats.
pub const MAX_EVENTS: usize = 8;
/// Event code of mouse button `n` (0..=4) is `BUTTON_CODE + n`; key codes are HID usages (page 0x07).
pub const BUTTON_CODE: u8 = 0xF0;
/// Bitmap of HID usages 0..=255.
pub const KEY_BYTES: usize = 32;

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
    pub const fn from_byte(byte: u8) -> Option<MouseMode> {
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

/// The whole input state; a lost packet is made good by the next one: running mouse totals, the last
/// `MAX_EVENTS` key / button events in order, and the bitmap of keys held.
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
