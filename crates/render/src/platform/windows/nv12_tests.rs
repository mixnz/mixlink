use windows::Win32::Graphics::Direct3D11::ID3D11RenderTargetView;

use mixlink_core::video::ColorMatrix;

use super::Nv12Pass;
use crate::VideoFrame;
use crate::platform::windows::test_util::{bgra_target, nv12_bands, read_pixel, warp_device};

const WHITE: (u8, u8, u8) = (235, 128, 128);
const BLACK: (u8, u8, u8) = (16, 128, 128);
const RED: (u8, u8, u8) = (63, 102, 240);
/// Pure red in BT.601 limited range (white, black and grey are the same in both matrices).
const RED_601: (u8, u8, u8) = (82, 90, 240);
const GREY: (u8, u8, u8) = (126, 128, 128);

fn close(got: (u8, u8, u8), want: (u8, u8, u8)) -> bool {
    let d = |a: u8, b: u8| a.abs_diff(b) <= 2;
    d(got.0, want.0) && d(got.1, want.1) && d(got.2, want.2)
}

/// Draws an NV12 source of `src` size with `bands` into a 64x64 target; returns pixel reader.
fn render(src: (u32, u32), bands: &[(u8, u8, u8)]) -> impl Fn(u32, u32) -> (u8, u8, u8) {
    render_with(src, bands, ColorMatrix::Bt709)
}

fn render_with(
    src: (u32, u32),
    bands: &[(u8, u8, u8)],
    matrix: ColorMatrix,
) -> impl Fn(u32, u32) -> (u8, u8, u8) {
    let (device, context) = warp_device();
    let source = nv12_bands(&device, src.0, src.1, bands);
    let target = bgra_target(&device, 64, 64);
    let mut rtv: Option<ID3D11RenderTargetView> = None;
    // SAFETY: `target` is a live render-target texture; valid out-pointer.
    unsafe { device.CreateRenderTargetView(&target, None, Some(&mut rtv)) }.unwrap();
    let mut pass = Nv12Pass::new(&device).unwrap();
    let frame = VideoFrame {
        texture: &source,
        array_index: 0,
        width: src.0,
        height: src.1,
        matrix,
    };
    pass.draw(&context, &frame, &rtv.unwrap(), (64, 64))
        .unwrap();
    move |x, y| read_pixel(&device, &context, &target, (64, 64), (x, y))
}

#[test]
fn converts_bt709_limited_range() {
    let pixel = render((64, 64), &[WHITE, BLACK, RED, GREY]);
    let cases = [
        (8, (255, 255, 255), "white"),
        (24, (0, 0, 0), "black"),
        (40, (255, 0, 0), "red"),
        (56, (128, 128, 128), "grey"),
    ];
    for (y, want, name) in cases {
        let got = pixel(32, y);
        assert!(close(got, want), "{name}: got {got:?}, want {want:?}");
    }
}

#[test]
fn converts_bt601_limited_range() {
    let pixel = render_with((64, 64), &[WHITE, RED_601, GREY, BLACK], ColorMatrix::Bt601);
    let cases = [
        (8, (255, 255, 255), "white"),
        (24, (255, 0, 0), "red"),
        (40, (128, 128, 128), "grey"),
        (56, (0, 0, 0), "black"),
    ];
    for (y, want, name) in cases {
        let got = pixel(32, y);
        assert!(close(got, want), "{name}: got {got:?}, want {want:?}");
    }
}

#[test]
fn matrix_switches_between_frames() {
    // The same 601-coded red drawn as 709 is visibly off: the pass must not cache the first matrix.
    let as_709 = render_with((64, 64), &[RED_601], ColorMatrix::Bt709)(32, 32);
    let as_601 = render_with((64, 64), &[RED_601], ColorMatrix::Bt601)(32, 32);
    assert!(close(as_601, (255, 0, 0)), "601: got {as_601:?}");
    assert!(
        !close(as_709, (255, 0, 0)),
        "709 should differ, got {as_709:?}"
    );
}

#[test]
fn letterbox_bars_are_black() {
    let pixel = render((64, 32), &[WHITE]);
    for y in [0, 8, 15, 48, 56, 63] {
        assert!(close(pixel(32, y), (0, 0, 0)), "row {y} not black");
    }
    for y in [17, 32, 46] {
        assert!(close(pixel(32, y), (255, 255, 255)), "row {y} not white");
    }
}
