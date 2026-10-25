use windows::Win32::Graphics::Direct3D11::ID3D11RenderTargetView;

use super::Nv12Pass;
use crate::VideoFrame;
use crate::platform::windows::test_util::{bgra_target, nv12_bands, read_pixel, warp_device};

const WHITE: (u8, u8, u8) = (235, 128, 128);
const BLACK: (u8, u8, u8) = (16, 128, 128);
const RED: (u8, u8, u8) = (63, 102, 240);
const GREY: (u8, u8, u8) = (126, 128, 128);

fn close(got: (u8, u8, u8), want: (u8, u8, u8)) -> bool {
    let d = |a: u8, b: u8| a.abs_diff(b) <= 2;
    d(got.0, want.0) && d(got.1, want.1) && d(got.2, want.2)
}

/// Draws an NV12 source of `src` size with `bands` into a 64x64 target; returns pixel reader.
fn render(src: (u32, u32), bands: &[(u8, u8, u8)]) -> impl Fn(u32, u32) -> (u8, u8, u8) {
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
fn letterbox_bars_are_black() {
    let pixel = render((64, 32), &[WHITE]);
    for y in [0, 8, 15, 48, 56, 63] {
        assert!(close(pixel(32, y), (0, 0, 0)), "row {y} not black");
    }
    for y in [17, 32, 46] {
        assert!(close(pixel(32, y), (255, 255, 255)), "row {y} not white");
    }
}
