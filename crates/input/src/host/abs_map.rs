//! Stream pixel → `vhid` absolute position (spec 0021). Windows maps the absolute mouse to the main
//! display (measured with `extend`: a 4K monitor to the right of the virtual one, the pointer stayed on
//! the virtual one), and the client's virtual monitor is the main display while it streams.

/// Absolute report range on each axis.
const ABS_MAX: u32 = 32_767;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AbsMap {
    /// Last pixel on each axis (width − 1, height − 1), at least 1.
    last: (u32, u32),
}

impl AbsMap {
    pub fn new(stream: (u32, u32)) -> AbsMap {
        AbsMap {
            last: (
                stream.0.saturating_sub(1).max(1),
                stream.1.saturating_sub(1).max(1),
            ),
        }
    }

    /// `x`, `y` in stream pixels → absolute report units, clamped.
    // HOT PATH
    pub fn map(&self, x: u16, y: u16) -> (u16, u16) {
        let axis = |v: u16, last: u32| (u32::from(v).min(last) * ABS_MAX / last) as u16;
        (axis(x, self.last.0), axis(y, self.last.1))
    }
}

#[cfg(test)]
#[path = "abs_map_tests.rs"]
mod tests;
