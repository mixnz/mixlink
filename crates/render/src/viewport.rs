//! Where a video frame goes in the window: scaled to fit, aspect ratio kept, centred.

/// A rectangle in window pixels.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// Largest rectangle with the aspect ratio of `src` that fits `dst`, centred (black bars around).
/// Any zero size gives an empty rectangle.
pub fn fit(src: (u32, u32), dst: (u32, u32)) -> Rect {
    if src.0 == 0 || src.1 == 0 || dst.0 == 0 || dst.1 == 0 {
        return Rect::default();
    }
    let (sw, sh) = (src.0 as f32, src.1 as f32);
    let (dw, dh) = (dst.0 as f32, dst.1 as f32);
    // The limiting axis takes the window size exactly (no rounding drift at the edges).
    let (w, h) = if dw * sh <= dh * sw {
        (dw, sh * dw / sw)
    } else {
        (sw * dh / sh, dh)
    };
    Rect {
        x: (dw - w) / 2.0,
        y: (dh - h) / 2.0,
        w,
        h,
    }
}

#[cfg(test)]
#[path = "viewport_tests.rs"]
mod tests;
