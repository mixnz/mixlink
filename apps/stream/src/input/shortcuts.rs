//! Stream window shortcuts (spec 0021): Ctrl+Alt+Shift (either side) + Z releases input, + F toggles
//! fullscreen, + Q closes, + L locks the host. The letter never reaches the host; the modifiers already did
//! (the caller releases them on the host before it acts). Win+L locks this PC (Windows acts on it before
//! any hook): its L is kept here and the host gets a Ctrl tap, so the held Win does not open its Start.

const L_CTRL: u8 = 0xE0;
const L_SHIFT: u8 = 0xE1;
const L_ALT: u8 = 0xE2;
const R_CTRL: u8 = 0xE4;
const R_SHIFT: u8 = 0xE5;
const R_ALT: u8 = 0xE6;
const KEY_Z: u8 = 0x1D;
const KEY_F: u8 = 0x09;
const KEY_Q: u8 = 0x14;
const KEY_L: u8 = 0x0F;
const L_WIN: u8 = 0xE3;
const R_WIN: u8 = 0xE7;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shortcut {
    Release,
    Fullscreen,
    Close,
    /// Win+L on the host (handled by `stream-input`).
    LockHost,
}

/// What to do with one key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyUse {
    /// Send it to the host.
    Forward,
    /// Keep it here (the release of a shortcut's letter).
    Drop,
    /// Keep it here and tap Ctrl on the host (Win+L: this PC locks, the host must not open Start).
    MaskWin,
    Run(Shortcut),
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Shortcuts {
    /// Modifier usages 0xE0..=0xE7 held, bit = usage − 0xE0.
    modifiers: u8,
    /// A shortcut letter whose release is kept here.
    eaten: Option<u8>,
}

impl Shortcuts {
    // HOT PATH
    pub fn key(&mut self, usage: u8, down: bool) -> KeyUse {
        if (L_CTRL..=0xE7).contains(&usage) {
            let bit = 1u8 << (usage - L_CTRL);
            if down {
                self.modifiers |= bit;
            } else {
                self.modifiers &= !bit;
            }
            return KeyUse::Forward;
        }
        if !down && self.eaten == Some(usage) {
            self.eaten = None;
            return KeyUse::Drop;
        }
        if down && usage == KEY_L && self.held(L_WIN, R_WIN) {
            self.eaten = Some(usage);
            return KeyUse::MaskWin;
        }
        if !down || !self.combo_held() {
            return KeyUse::Forward;
        }
        let shortcut = match usage {
            KEY_Z => Shortcut::Release,
            KEY_F => Shortcut::Fullscreen,
            KEY_Q => Shortcut::Close,
            KEY_L => Shortcut::LockHost,
            _ => return KeyUse::Forward,
        };
        self.eaten = Some(usage);
        KeyUse::Run(shortcut)
    }

    /// Forgets held keys (input released: their key-ups go to this PC, not here).
    pub fn reset(&mut self) {
        *self = Shortcuts::default();
    }

    fn combo_held(&self) -> bool {
        self.held(L_CTRL, R_CTRL) && self.held(L_ALT, R_ALT) && self.held(L_SHIFT, R_SHIFT)
    }

    /// Either side of a modifier is down.
    fn held(&self, left: u8, right: u8) -> bool {
        self.modifiers & (1 << (left - L_CTRL) | 1 << (right - L_CTRL)) != 0
    }
}

#[cfg(test)]
#[path = "shortcuts_tests.rs"]
mod tests;
