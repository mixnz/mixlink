//! Frame counter drawn by `s2-pattern` in the top-left cell: 32 blocks of 8x8 px, bit 31 first,
//! white = 1, black = 0. Pixels are BGRA8.

pub const BLOCK: u32 = 8;
pub const CELL_W: u32 = BLOCK * 32;
pub const CELL_H: u32 = BLOCK;

const ONE_ABOVE: u32 = 192;
const ZERO_BELOW: u32 = 64;

pub fn encode(n: u32) -> [bool; 32] {
    let mut bits = [false; 32];
    for (i, bit) in bits.iter_mut().enumerate() {
        *bit = (n >> (31 - i)) & 1 == 1;
    }
    bits
}

/// Reads the centre pixel of each block. `None` when a block is neither clearly white nor
/// clearly black (scaled, wrong monitor, or not the pattern at all).
pub fn decode(bgra: &[u8], row_pitch: usize) -> Option<u32> {
    let y = (BLOCK / 2) as usize;
    let mut n = 0u32;
    for i in 0..32usize {
        let x = i * BLOCK as usize + (BLOCK / 2) as usize;
        let p = y * row_pitch + x * 4;
        let px = bgra.get(p..p + 3)?;
        let luma = (u32::from(px[0]) + u32::from(px[1]) + u32::from(px[2])) / 3;
        let bit = if luma > ONE_ABOVE {
            1
        } else if luma < ZERO_BELOW {
            0
        } else {
            return None;
        };
        n = (n << 1) | bit;
    }
    Some(n)
}

#[cfg(test)]
#[path = "counter_tests.rs"]
mod tests;
