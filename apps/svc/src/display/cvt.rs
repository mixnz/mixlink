//! CVT reduced blanking (v1) timings for the EDID's detailed timing (spec 0015).

/// Minimum vertical blanking time, µs.
const MIN_VBLANK_US: f64 = 460.0;
const H_BLANK: u32 = 160;
const H_FRONT: u32 = 48;
const H_SYNC: u32 = 32;
const V_FRONT: u32 = 3;
const MIN_V_BACK: u32 = 6;
/// The pixel clock is a multiple of this.
const CLOCK_STEP_KHZ: u32 = 250;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timing {
    pub pixel_khz: u32,
    pub h_active: u32,
    pub h_blank: u32,
    pub h_front: u32,
    pub h_sync: u32,
    pub v_active: u32,
    pub v_blank: u32,
    pub v_front: u32,
    pub v_sync: u32,
}

/// Vertical sync width by aspect ratio (CVT 1.2 table 3-2); 10 for non-standard aspects.
fn v_sync_for(width: u32, height: u32) -> u32 {
    let (w, h) = (u64::from(width), u64::from(height));
    if w * 3 == h * 4 {
        4
    } else if w * 9 == h * 16 {
        5
    } else if w * 10 == h * 16 {
        6
    } else if w * 4 == h * 5 || w * 9 == h * 15 {
        7
    } else {
        10
    }
}

/// Inputs come from validated modes (`width`, `height` > 0, refresh ≥ 1 Hz).
pub fn cvt_rb(width: u32, height: u32, refresh_mhz: u32) -> Timing {
    let hz = f64::from(refresh_mhz) / 1000.0;
    let h_period_est = (1_000_000.0 / hz - MIN_VBLANK_US) / f64::from(height.max(1));
    let vbi = (MIN_VBLANK_US / h_period_est).floor() as u32 + 1;
    let v_sync = v_sync_for(width, height);
    let v_blank = vbi.max(V_FRONT + v_sync + MIN_V_BACK);
    let total = f64::from(width + H_BLANK) * f64::from(height + v_blank);
    let steps = (hz * total / 1000.0 / f64::from(CLOCK_STEP_KHZ)).floor() as u32;
    Timing {
        pixel_khz: steps * CLOCK_STEP_KHZ,
        h_active: width,
        h_blank: H_BLANK,
        h_front: H_FRONT,
        h_sync: H_SYNC,
        v_active: height,
        v_blank,
        v_front: V_FRONT,
        v_sync,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rb_1080p60() {
        let t = cvt_rb(1920, 1080, 60_000);
        assert_eq!(t.pixel_khz, 138_500);
        assert_eq!(t.h_active + t.h_blank, 2080);
        assert_eq!(t.v_active + t.v_blank, 1111);
        assert_eq!((t.h_front, t.h_sync), (48, 32));
        assert_eq!((t.v_front, t.v_sync), (3, 5));
    }

    #[test]
    fn rb_1440p60() {
        let t = cvt_rb(2560, 1440, 60_000);
        assert_eq!(t.pixel_khz, 241_500);
        assert_eq!(t.h_active + t.h_blank, 2720);
        assert_eq!(t.v_active + t.v_blank, 1481);
    }

    #[test]
    fn rb_sync_by_aspect() {
        assert_eq!(cvt_rb(1024, 768, 60_000).v_sync, 4);
        assert_eq!(cvt_rb(1920, 1200, 60_000).v_sync, 6);
        assert_eq!(cvt_rb(1280, 1024, 60_000).v_sync, 7);
        assert_eq!(cvt_rb(3440, 1440, 60_000).v_sync, 10);
    }
}
