//! Mouse reports (spec 0016). Relative: buttons, X/Y i16, wheel i8, AC Pan i8. Absolute: buttons,
//! X/Y 0..=32767.

use super::report::{PAYLOAD_MAX, Report, Target};

/// Largest relative step one report carries (logical range -32767..=32767).
pub const REL_MAX: i32 = 32_767;
/// Reports one relative move may split into; larger moves are capped.
const REL_REPORTS: usize = 4;
const ABS_MAX: u16 = 32_767;
/// Buttons 1..=5.
const BUTTON_MASK: u8 = 0x1F;

/// Up to four relative reports for one move.
#[derive(Debug, Clone, Copy)]
pub struct RelReports {
    reports: [Report; REL_REPORTS],
    len: usize,
}

impl RelReports {
    pub fn as_slice(&self) -> &[Report] {
        &self.reports[..self.len]
    }
}

/// Splits `dx` / `dy` into steps of at most `REL_MAX` each (four reports at most, the rest is
/// dropped); wheel and pan go in the first report only. Always at least one report.
// HOT PATH
pub fn mouse_rel_reports(buttons: u8, dx: i32, dy: i32, wheel: i8, pan: i8) -> RelReports {
    let empty = Report::new(Target::MouseRel, [0; PAYLOAD_MAX]);
    let mut out = RelReports {
        reports: [empty; REL_REPORTS],
        len: 0,
    };
    let (mut rest_x, mut rest_y) = (dx, dy);
    while out.len < REL_REPORTS && (out.len == 0 || rest_x != 0 || rest_y != 0) {
        let step_x = rest_x.clamp(-REL_MAX, REL_MAX);
        let step_y = rest_y.clamp(-REL_MAX, REL_MAX);
        rest_x -= step_x;
        rest_y -= step_y;
        let mut payload = [0u8; PAYLOAD_MAX];
        payload[0] = buttons & BUTTON_MASK;
        payload[1..3].copy_from_slice(&to_i16(step_x).to_le_bytes());
        payload[3..5].copy_from_slice(&to_i16(step_y).to_le_bytes());
        if out.len == 0 {
            payload[5] = wheel.to_le_bytes()[0];
            payload[6] = pan.to_le_bytes()[0];
        }
        out.reports[out.len] = Report::new(Target::MouseRel, payload);
        out.len += 1;
    }
    out
}

/// `x` / `y` in 0..=32767 across the virtual desktop (clamped).
// HOT PATH
pub fn mouse_abs_report(buttons: u8, x: u16, y: u16) -> Report {
    let mut payload = [0u8; PAYLOAD_MAX];
    payload[0] = buttons & BUTTON_MASK;
    payload[1..3].copy_from_slice(&x.min(ABS_MAX).to_le_bytes());
    payload[3..5].copy_from_slice(&y.min(ABS_MAX).to_le_bytes());
    Report::new(Target::MouseAbs, payload)
}

/// `value` is already within ±`REL_MAX`.
fn to_i16(value: i32) -> i16 {
    i16::try_from(value).unwrap_or(0)
}
