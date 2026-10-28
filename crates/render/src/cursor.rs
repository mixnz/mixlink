//! The host cursor drawn over the video (spec 0020): shapes become premultiplied BGRA images (plus an
//! invert mask for XOR pixels), placed with the video's aspect fit. Pure; the draw pass is per platform.

use crate::RenderError;
use crate::viewport::{Rect, fit};

/// Largest side the host sends (the driver offers 256 to IddCx).
pub const CURSOR_MAX_SIDE: u32 = 256;

/// Pixel format of a host shape, 32 bpp BGRA either way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorKind {
    /// Straight alpha.
    Alpha,
    /// Alpha byte is a mask: 0xFF = opaque colour, 0 = XOR with the screen (IddCx; the reverse of DXGI
    /// duplication, seen on the text cursor in spec 0020 Results).
    MaskedColor,
}

/// A shape ready for the GPU: tightly packed, premultiplied BGRA.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CursorImage {
    pub width: u32,
    pub height: u32,
    pub hot: (u32, u32),
    pub normal: Vec<u8>,
    /// White where the shape XORs a non-black colour, drawn as "invert what is below" (XOR approximated).
    pub invert: Option<Vec<u8>>,
}

/// Where and whether to draw the cursor: hotspot in video pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CursorDraw {
    pub x: i32,
    pub y: i32,
    pub visible: bool,
}

/// Converts a host shape (rows of `pitch` bytes). Allocates: call on a shape change, not per frame.
pub fn convert_shape(
    kind: CursorKind,
    width: u32,
    height: u32,
    hot: (u32, u32),
    pitch: u32,
    pixels: &[u8],
) -> Result<CursorImage, RenderError> {
    let sides = 1..=CURSOR_MAX_SIDE;
    let need = u64::from(pitch) * u64::from(height);
    if !sides.contains(&width)
        || !sides.contains(&height)
        || u64::from(pitch) < u64::from(width) * 4
        || (pixels.len() as u64) < need
    {
        return Err(RenderError::CursorShape {
            width,
            height,
            pitch,
            len: pixels.len(),
        });
    }
    let row_bytes = width as usize * 4;
    let mut normal = Vec::with_capacity(row_bytes * height as usize);
    let mut invert = vec![0u8; row_bytes * height as usize];
    let mut inverts = false;
    for row in pixels.chunks(pitch as usize).take(height as usize) {
        for px in row[..row_bytes].as_chunks::<4>().0 {
            let (b, g, r, a) = (px[0], px[1], px[2], px[3]);
            let out = match kind {
                CursorKind::Alpha => [mul(b, a), mul(g, a), mul(r, a), a],
                CursorKind::MaskedColor if a == 0xFF => [b, g, r, 255],
                CursorKind::MaskedColor if (b, g, r) == (0, 0, 0) => [0; 4],
                CursorKind::MaskedColor => {
                    let at = normal.len();
                    invert[at..at + 4].copy_from_slice(&[255; 4]);
                    inverts = true;
                    [0; 4]
                }
            };
            normal.extend_from_slice(&out);
        }
    }
    Ok(CursorImage {
        width,
        height,
        hot,
        normal,
        invert: inverts.then_some(invert),
    })
}

fn mul(c: u8, a: u8) -> u8 {
    ((u16::from(c) * u16::from(a) + 127) / 255) as u8
}

/// The cursor image's rectangle in window pixels: the video is fitted into `dst` (black bars), the image
/// scales with it and its hotspot lands on `at`.
pub fn cursor_rect(
    video: (u32, u32),
    dst: (u32, u32),
    size: (u32, u32),
    hot: (u32, u32),
    at: (i32, i32),
) -> Rect {
    let area = fit(video, dst);
    if area.w <= 0.0 || video.0 == 0 {
        return Rect::default();
    }
    let scale = area.w / video.0 as f32;
    Rect {
        x: area.x + (at.0 as f32 - hot.0 as f32) * scale,
        y: area.y + (at.1 as f32 - hot.1 as f32) * scale,
        w: size.0 as f32 * scale,
        h: size.1 as f32 * scale,
    }
}

#[cfg(test)]
#[path = "cursor_tests.rs"]
mod tests;
