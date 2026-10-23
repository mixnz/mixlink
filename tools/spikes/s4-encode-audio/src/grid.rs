//! Frame number as a 8 x 4 grid of 64 px blocks (32 bits, LSB first), readable after encode + decode.

pub const BLOCK: u32 = 64;
pub const COLS: u32 = 8;
pub const ROWS: u32 = 4;
/// Luma above this reads as bit 1.
const THRESHOLD: u32 = 128;
/// Half-size of the sampled square at the block centre (edges blur after compression).
const SAMPLE_HALF: u32 = 8;

/// Top-left corner and bit value of each block for frame number `n`.
pub fn draw_rects(n: u32) -> [(u32, u32, bool); 32] {
    let mut rects = [(0, 0, false); 32];
    for (bit, rect) in rects.iter_mut().enumerate() {
        let bit = bit as u32;
        *rect = (
            (bit % COLS) * BLOCK,
            (bit / COLS) * BLOCK,
            n >> bit & 1 == 1,
        );
    }
    rects
}

/// Reads the frame number from a luma plane (at least 512 x 256 px).
pub fn read(luma: &[u8], stride: usize) -> u32 {
    let mut n = 0u32;
    for (bit, (x, y, _)) in draw_rects(0).iter().enumerate() {
        let cx = x + BLOCK / 2;
        let cy = y + BLOCK / 2;
        let mut sum = 0u32;
        let mut count = 0u32;
        for row in cy - SAMPLE_HALF..cy + SAMPLE_HALF {
            let start = row as usize * stride;
            for col in cx - SAMPLE_HALF..cx + SAMPLE_HALF {
                sum += u32::from(luma.get(start + col as usize).copied().unwrap_or(0));
                count += 1;
            }
        }
        if sum / count > THRESHOLD {
            n |= 1 << bit;
        }
    }
    n
}

#[cfg(test)]
#[path = "grid_tests.rs"]
mod tests;
