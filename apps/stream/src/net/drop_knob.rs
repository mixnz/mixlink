//! Dev loss knob (spec 0020): `--drop video=0.1,cursor=5` drops that percentage (to 0.01 %) of received
//! datagrams before they are opened, so loss recovery can be tried on a clean LAN. Off by default.

use crate::args::ArgsError;

/// Which lane a datagram came in on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropLane {
    Video,
    Cursor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DropKnob {
    /// Hundredths of a percent (basis points): 10 = 0.1 %.
    pub video_bp: u16,
    pub cursor_bp: u16,
    /// xorshift32 state; never 0.
    state: u32,
}

impl Default for DropKnob {
    fn default() -> Self {
        Self::new(0, 0)
    }
}

impl DropKnob {
    pub const fn new(video_bp: u16, cursor_bp: u16) -> Self {
        Self {
            video_bp,
            cursor_bp,
            state: 0x9E37_79B9,
        }
    }

    /// `video=0.1,cursor=5` (either part may be left out); percentages 0..=100, two decimals kept.
    pub fn parse(text: &str) -> Result<DropKnob, ArgsError> {
        let bad = || ArgsError::BadDrop(text.to_owned());
        let mut knob = DropKnob::default();
        for part in text.split(',') {
            let (lane, pct) = part.split_once('=').ok_or_else(bad)?;
            let pct = pct
                .trim()
                .parse::<f64>()
                .ok()
                .filter(|p| (0.0..=100.0).contains(p))
                .ok_or_else(bad)?;
            let bp = (pct * 100.0).round() as u16;
            match lane.trim() {
                "video" => knob.video_bp = bp,
                "cursor" => knob.cursor_bp = bp,
                _ => return Err(bad()),
            }
        }
        Ok(knob)
    }

    // HOT PATH
    pub fn should_drop(&mut self, lane: DropLane) -> bool {
        let bp = match lane {
            DropLane::Video => self.video_bp,
            DropLane::Cursor => self.cursor_bp,
        };
        if bp == 0 {
            return false;
        }
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.state = x;
        x % 10_000 < u32::from(bp)
    }
}

#[cfg(test)]
#[path = "drop_knob_tests.rs"]
mod tests;
