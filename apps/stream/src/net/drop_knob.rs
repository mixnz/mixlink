//! Dev loss knob (spec 0020): `--drop video=1,cursor=5` drops that percentage of received datagrams
//! before they are opened, so loss recovery can be tried on a clean LAN. Off by default.

use crate::args::ArgsError;

/// Which lane a datagram came in on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropLane {
    Video,
    Cursor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DropKnob {
    pub video_pct: u8,
    pub cursor_pct: u8,
    /// xorshift32 state; never 0.
    state: u32,
}

impl Default for DropKnob {
    fn default() -> Self {
        Self::new(0, 0)
    }
}

impl DropKnob {
    pub const fn new(video_pct: u8, cursor_pct: u8) -> Self {
        Self {
            video_pct,
            cursor_pct,
            state: 0x9E37_79B9,
        }
    }

    /// `video=1,cursor=5` (either part may be left out); percentages 0..=100.
    pub fn parse(text: &str) -> Result<DropKnob, ArgsError> {
        let bad = || ArgsError::BadDrop(text.to_owned());
        let mut knob = DropKnob::default();
        for part in text.split(',') {
            let (lane, pct) = part.split_once('=').ok_or_else(bad)?;
            let pct = pct
                .trim()
                .parse::<u8>()
                .ok()
                .filter(|p| *p <= 100)
                .ok_or_else(bad)?;
            match lane.trim() {
                "video" => knob.video_pct = pct,
                "cursor" => knob.cursor_pct = pct,
                _ => return Err(bad()),
            }
        }
        Ok(knob)
    }

    // HOT PATH
    pub fn should_drop(&mut self, lane: DropLane) -> bool {
        let pct = match lane {
            DropLane::Video => self.video_pct,
            DropLane::Cursor => self.cursor_pct,
        };
        if pct == 0 {
            return false;
        }
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.state = x;
        x % 100 < u32::from(pct)
    }
}

#[cfg(test)]
#[path = "drop_knob_tests.rs"]
mod tests;
