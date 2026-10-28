//! Stream pixel → `vhid` absolute position (spec 0021). The absolute mouse spans the whole virtual
//! desktop (0..=32767 on each axis), so the client's monitor is found inside the desktop's bounds.

/// Absolute report range on each axis.
const ABS_MAX: i64 = 32_767;

/// A rectangle in desktop pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

/// The client's monitor and the bounds of every active display.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DesktopPlace {
    pub monitor: Rect,
    pub desktop: Rect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AbsMap {
    stream: (u32, u32),
    place: DesktopPlace,
}

impl AbsMap {
    /// `place` `None`: the monitor is the whole desktop (`virtual-only`, or the layout is unknown).
    pub fn new(stream: (u32, u32), place: Option<DesktopPlace>) -> AbsMap {
        let stream = (stream.0.max(1), stream.1.max(1));
        let whole = Rect {
            x: 0,
            y: 0,
            w: stream.0,
            h: stream.1,
        };
        AbsMap {
            stream,
            place: place.unwrap_or(DesktopPlace {
                monitor: whole,
                desktop: whole,
            }),
        }
    }

    /// `x`, `y` in stream pixels → absolute report units, clamped.
    // HOT PATH
    pub fn map(&self, x: u16, y: u16) -> (u16, u16) {
        let DesktopPlace { monitor, desktop } = self.place;
        (
            axis(x, self.stream.0, monitor.x, monitor.w, desktop.x, desktop.w),
            axis(y, self.stream.1, monitor.y, monitor.h, desktop.y, desktop.h),
        )
    }
}

fn axis(v: u16, stream: u32, mon_at: i32, mon_len: u32, desk_at: i32, desk_len: u32) -> u16 {
    let px = i64::from(mon_at) + i64::from(v) * i64::from(mon_len) / i64::from(stream);
    let span = (i64::from(desk_len) - 1).max(1);
    let abs = (px - i64::from(desk_at)) * ABS_MAX / span;
    abs.clamp(0, ABS_MAX) as u16
}

#[cfg(test)]
#[path = "abs_map_tests.rs"]
mod tests;
