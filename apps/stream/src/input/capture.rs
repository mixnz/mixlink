//! Window pixel → stream pixel (spec 0021): the video is letterboxed in the window (`viewport::fit`); a
//! pointer over a black bar or past an edge sits on the nearest edge of the picture.

use mixlink_render::viewport::fit;

/// `None` only when the window or the stream has no size.
// HOT PATH
pub fn to_stream(pos: (f64, f64), window: (u32, u32), stream: (u32, u32)) -> Option<(u16, u16)> {
    let rect = fit(stream, window);
    if rect.w <= 0.0 || rect.h <= 0.0 {
        return None;
    }
    let axis = |p: f64, at: f32, len: f32, size: u32| -> u16 {
        let scaled = (p - f64::from(at)) * f64::from(size) / f64::from(len);
        let max = f64::from(size.saturating_sub(1).min(u32::from(u16::MAX)));
        scaled.floor().clamp(0.0, max) as u16
    };
    Some((
        axis(pos.0, rect.x, rect.w, stream.0),
        axis(pos.1, rect.y, rect.h, stream.1),
    ))
}

#[cfg(test)]
#[path = "capture_tests.rs"]
mod tests;
