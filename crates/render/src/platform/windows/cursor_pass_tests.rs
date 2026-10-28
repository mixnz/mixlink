use windows::Win32::Graphics::Direct3D11::ID3D11RenderTargetView;

use super::*;
use crate::cursor::{CursorKind, convert_shape};
use crate::platform::windows::test_util::{bgra_target, read_pixel, warp_device};

const SIZE: (u32, u32) = (64, 64);

/// Clears a 64x64 target to `background`, draws a 4x4 shape (all pixels `px`, BGRA) with its hotspot
/// at (10, 10); returns the pixel reader (RGB).
fn render(
    kind: CursorKind,
    px: [u8; 4],
    background: [f32; 4],
    visible: bool,
) -> impl Fn(u32, u32) -> (u8, u8, u8) {
    let (device, context) = warp_device();
    let target = bgra_target(&device, SIZE.0, SIZE.1);
    let mut rtv: Option<ID3D11RenderTargetView> = None;
    // SAFETY: `target` is a live render-target texture; valid out-pointer.
    unsafe { device.CreateRenderTargetView(&target, None, Some(&mut rtv)) }.unwrap();
    let rtv = rtv.unwrap();
    // SAFETY: live view.
    unsafe { context.ClearRenderTargetView(&rtv, &background) };
    let pixels: Vec<u8> = px.iter().copied().cycle().take(4 * 4 * 4).collect();
    let image = convert_shape(kind, 4, 4, (0, 0), 16, &pixels).unwrap();
    let mut pass = CursorPass::new(&device).unwrap();
    pass.set_image(&device, Some(&image)).unwrap();
    let at = CursorDraw {
        x: 10,
        y: 10,
        visible,
    };
    pass.draw(&context, &rtv, SIZE, SIZE, at);
    move |x, y| read_pixel(&device, &context, &target, SIZE, (x, y))
}

const BLACK: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
const WHITE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

#[test]
fn white_cursor_drawn_at_hotspot() {
    let pixel = render(CursorKind::Alpha, [255, 255, 255, 255], BLACK, true);
    assert_eq!(pixel(11, 11), (255, 255, 255));
    assert_eq!(pixel(10, 10), (255, 255, 255));
    assert_eq!(pixel(14, 14), (0, 0, 0), "outside the 4x4 shape");
    assert_eq!(pixel(0, 0), (0, 0, 0));
}

#[test]
fn transparent_pixels_keep_the_video() {
    let pixel = render(CursorKind::Alpha, [255, 255, 255, 0], WHITE, true);
    assert_eq!(pixel(11, 11), (255, 255, 255));
}

#[test]
fn invert_mask_over_white_is_black() {
    let pixel = render(CursorKind::MaskedColor, [255, 255, 255, 0xFF], WHITE, true);
    assert_eq!(pixel(11, 11), (0, 0, 0));
    assert_eq!(pixel(20, 20), (255, 255, 255));
}

#[test]
fn hidden_cursor_draws_nothing() {
    let pixel = render(CursorKind::Alpha, [255, 255, 255, 255], BLACK, false);
    assert_eq!(pixel(11, 11), (0, 0, 0));
}
