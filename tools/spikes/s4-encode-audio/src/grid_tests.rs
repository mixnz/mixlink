use super::*;

const W: usize = 512;
const H: usize = 256;

/// Paints the grid like the GPU does: Y 235 for 1, Y 16 for 0.
fn paint(n: u32) -> Vec<u8> {
    let mut luma = vec![16u8; W * H];
    for (x, y, bit) in draw_rects(n) {
        if !bit {
            continue;
        }
        for row in y..y + BLOCK {
            let start = row as usize * W;
            luma[start + x as usize..start + (x + BLOCK) as usize].fill(235);
        }
    }
    luma
}

#[test]
fn draw_rects_layout() {
    let rects = draw_rects(0b101);
    assert_eq!(rects[0], (0, 0, true));
    assert_eq!(rects[1], (64, 0, false));
    assert_eq!(rects[2], (128, 0, true));
    assert_eq!(rects[8], (0, 64, false));
    assert_eq!(rects[31], (448, 192, false));
}

#[test]
fn grid_roundtrip() {
    for n in [0, 1, 0xDEAD_BEEF, u32::MAX] {
        assert_eq!(read(&paint(n), W), n, "n = {n:#x}");
    }
}

#[test]
fn grid_roundtrip_survives_noise() {
    let n = 0x1234_5678;
    let mut luma = paint(n);
    // Deterministic +-40 noise (LCG), as compression artefacts would add.
    let mut state = 12_345u32;
    for px in luma.iter_mut() {
        state = state.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        let noise = (state >> 16) % 81;
        *px = (i32::from(*px) + noise as i32 - 40).clamp(0, 255) as u8;
    }
    assert_eq!(read(&luma, W), n);
}
