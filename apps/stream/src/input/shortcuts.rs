//! Stream window shortcuts (spec 0021): Ctrl+Alt+Shift (either side) + Z releases input, + F toggles
//! fullscreen, + Q closes. The letter never reaches the host; the modifiers already did (the caller
//! releases them on the host before it acts).

const L_CTRL: u8 = 0xE0;
const L_SHIFT: u8 = 0xE1;
const L_ALT: u8 = 0xE2;
const R_CTRL: u8 = 0xE4;
const R_SHIFT: u8 = 0xE5;
const R_ALT: u8 = 0xE6;
const KEY_Z: u8 = 0x1D;
const KEY_F: u8 = 0x09;
const KEY_Q: u8 = 0x14;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shortcut {
    Release,
    Fullscreen,
    Close,
}

/// What to do with one key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyUse {
    /// Send it to the host.
    Forward,
    /// Keep it here (the release of a shortcut's letter).
    Drop,
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
        if !down || !self.combo_held() {
            return KeyUse::Forward;
        }
        let shortcut = match usage {
            KEY_Z => Shortcut::Release,
            KEY_F => Shortcut::Fullscreen,
            KEY_Q => Shortcut::Close,
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
        let held = |left: u8, right: u8| {
            self.modifiers & (1 << (left - L_CTRL) | 1 << (right - L_CTRL)) != 0
        };
        held(L_CTRL, R_CTRL) && held(L_ALT, R_ALT) && held(L_SHIFT, R_SHIFT)
    }
}

#[cfg(test)]
#[path = "shortcuts_tests.rs"]
mod tests;
