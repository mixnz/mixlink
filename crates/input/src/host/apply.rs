//! Client input packets → `vhid` reports (spec 0021). Packets are whole states: an old `seq` is
//! ignored; events not applied yet are replayed in order; the key bitmap fixes what fell out of the
//! event ring; mouse totals give the motion since the last packet. Buttons go on the relative mouse
//! collection in every mode, so a press and its release always reach the same device.

use mixlink_core::input::{BUTTON_CODE, InputPacket, KEY_BYTES, MouseMode};

use super::abs_map::AbsMap;
use crate::hid::keyboard::KeyboardState;
use crate::hid::mouse::{mouse_abs_report, mouse_rel_reports};
use crate::hid::report::ReportSink;

/// 1/120 notch per wheel unit.
const NOTCH: i32 = 120;
const BUTTONS: u8 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Applied {
    /// Already applied (a repeat, or late).
    Stale,
    Fresh,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ApplyCounts {
    pub packets: u64,
    pub stale: u64,
    /// Events applied beyond the newest one of their packet: they were in a lost packet.
    pub recovered_events: u64,
    /// Times the keyboard was set from the bitmap (more events lost than a packet repeats).
    pub bitmap_fixes: u64,
    /// Mouse diagnostics: reports fed per collection, button presses and releases, mode switches.
    pub abs_reports: u64,
    pub rel_reports: u64,
    pub button_downs: u64,
    pub button_ups: u64,
    pub mode_changes: u64,
    /// Absolute position (stream pixels) at the last button releases, newest first.
    pub ups_at: [Option<(u16, u16)>; 4],
}

/// Totals of the last packet: rel x, rel y, wheel, hwheel.
#[derive(Debug, Clone, Copy)]
struct Totals([i32; 4]);

#[derive(Debug)]
pub struct InputApplier {
    map: AbsMap,
    last_seq: Option<u32>,
    last_event: u32,
    keyboard: KeyboardState,
    buttons: u8,
    totals: Option<Totals>,
    /// Wheel units not yet a whole notch (v, h).
    wheel_rest: (i32, i32),
    last_abs: Option<(u16, u16)>,
    last_mode: MouseMode,
    counts: ApplyCounts,
}

impl InputApplier {
    pub fn new(map: AbsMap) -> Self {
        Self {
            map,
            last_seq: None,
            last_event: 0,
            keyboard: KeyboardState::default(),
            buttons: 0,
            totals: None,
            wheel_rest: (0, 0),
            last_abs: None,
            last_mode: MouseMode::None,
            counts: ApplyCounts::default(),
        }
    }

    // HOT PATH
    pub fn apply<S: ReportSink>(
        &mut self,
        p: &InputPacket,
        sink: &mut S,
    ) -> Result<Applied, S::Error> {
        if self.last_seq.is_some_and(|last| !newer(p.seq, last)) {
            self.counts.stale += 1;
            return Ok(Applied::Stale);
        }
        self.last_seq = Some(p.seq);
        self.counts.packets += 1;
        if p.mode != self.last_mode {
            self.last_mode = p.mode;
            self.counts.mode_changes += 1;
        }
        if p.mode == MouseMode::Absolute && self.last_abs != Some((p.abs_x, p.abs_y)) {
            self.last_abs = Some((p.abs_x, p.abs_y));
            let (x, y) = self.map.map(p.abs_x, p.abs_y);
            sink.feed(&mouse_abs_report(0, x, y))?;
            self.counts.abs_reports += 1;
        }
        self.replay_events(p, sink)?;
        self.reconcile(p, sink)?;
        self.motion(p, sink)?;
        Ok(Applied::Fresh)
    }

    /// Releases every key and button on the host.
    pub fn release_all<S: ReportSink>(&mut self, sink: &mut S) -> Result<(), S::Error> {
        self.keyboard.clear();
        self.buttons = 0;
        sink.feed(&self.keyboard.report())?;
        self.feed_buttons(sink)
    }

    pub fn held(&self) -> bool {
        self.buttons != 0 || self.keyboard != KeyboardState::default()
    }

    pub fn counts(&self) -> ApplyCounts {
        self.counts
    }

    fn replay_events<S: ReportSink>(
        &mut self,
        p: &InputPacket,
        sink: &mut S,
    ) -> Result<(), S::Error> {
        let mut applied = 0u64;
        for (i, event) in p.events.iter().take(usize::from(p.count)).enumerate() {
            let number = p.first_event.wrapping_add(i as u32);
            if !newer(number, self.last_event) {
                continue;
            }
            self.last_event = number;
            applied += 1;
            if event.code >= BUTTON_CODE {
                let n = event.code - BUTTON_CODE;
                if n < BUTTONS {
                    set_bit(&mut self.buttons, n, event.down);
                    self.count_button(event.down);
                    self.feed_buttons(sink)?;
                }
            } else if self.keyboard.set(event.code, event.down) {
                sink.feed(&self.keyboard.report())?;
            }
        }
        self.counts.recovered_events += applied.saturating_sub(1);
        Ok(())
    }

    fn reconcile<S: ReportSink>(&mut self, p: &InputPacket, sink: &mut S) -> Result<(), S::Error> {
        let want = keyboard_from(&p.keys);
        if want != self.keyboard {
            self.keyboard = want;
            self.counts.bitmap_fixes += 1;
            sink.feed(&self.keyboard.report())?;
        }
        let buttons = p.buttons & ((1 << BUTTONS) - 1);
        if buttons != self.buttons {
            self.buttons = buttons;
            self.feed_buttons(sink)?;
        }
        Ok(())
    }

    fn motion<S: ReportSink>(&mut self, p: &InputPacket, sink: &mut S) -> Result<(), S::Error> {
        let now = Totals([p.rel_x, p.rel_y, p.wheel, p.hwheel]);
        let Some(before) = self.totals.replace(now) else {
            return Ok(());
        };
        let delta = |i: usize| now.0[i].wrapping_sub(before.0[i]);
        let (wheel, hwheel) = (
            self.wheel_rest.0.saturating_add(delta(2)),
            self.wheel_rest.1.saturating_add(delta(3)),
        );
        let notches = (notch(wheel), notch(hwheel));
        self.wheel_rest = (
            wheel - i32::from(notches.0) * NOTCH,
            hwheel - i32::from(notches.1) * NOTCH,
        );
        let (dx, dy) = match p.mode {
            MouseMode::Relative => (delta(0), delta(1)),
            MouseMode::Absolute | MouseMode::None => (0, 0),
        };
        if dx == 0 && dy == 0 && notches == (0, 0) {
            return Ok(());
        }
        for report in mouse_rel_reports(self.buttons, dx, dy, notches.0, notches.1).as_slice() {
            sink.feed(report)?;
            self.counts.rel_reports += 1;
        }
        Ok(())
    }

    fn feed_buttons<S: ReportSink>(&mut self, sink: &mut S) -> Result<(), S::Error> {
        self.counts.rel_reports += 1;
        sink.feed(&mouse_rel_reports(self.buttons, 0, 0, 0, 0).as_slice()[0])
    }

    fn count_button(&mut self, down: bool) {
        if down {
            self.counts.button_downs += 1;
        } else {
            self.counts.button_ups += 1;
            self.counts.ups_at.rotate_right(1);
            self.counts.ups_at[0] = self.last_abs;
        }
    }
}

/// RFC 1982 order on u32: `a` comes after `b`.
fn newer(a: u32, b: u32) -> bool {
    (a.wrapping_sub(b) as i32) > 0
}

fn set_bit(bits: &mut u8, n: u8, on: bool) {
    if on {
        *bits |= 1 << n;
    } else {
        *bits &= !(1 << n);
    }
}

/// Whole notches in `units` (toward zero), as the report's i8.
fn notch(units: i32) -> i8 {
    (units / NOTCH).clamp(i32::from(i8::MIN), i32::from(i8::MAX)) as i8
}

fn keyboard_from(keys: &[u8; KEY_BYTES]) -> KeyboardState {
    let mut state = KeyboardState::default();
    for (byte, &bits) in keys.iter().enumerate() {
        for bit in 0..8 {
            if bits & (1 << bit) != 0 {
                state.set((byte * 8 + bit) as u8, true);
            }
        }
    }
    state
}

#[cfg(test)]
#[path = "apply_tests.rs"]
mod tests;
