//! S2b latency levers (spec 0005). Bits match `drivers/vdisplay/Levers.h`.

use std::fmt::Write as _;

use crate::header::S2Header;

pub const L1_CPU: u32 = 0x1;
pub const L2_GPU: u32 = 0x2;
pub const L3_NO_COPY: u32 = 0x4;
pub const L4_D3D12: u32 = 0x8;

const ALL: [u32; 4] = [L1_CPU, L2_GPU, L3_NO_COPY, L4_D3D12];

/// `"none"` for 0, else `"L1+L3"` style, lowest lever first.
pub fn lever_names(mask: u32) -> String {
    let names: Vec<String> = ALL
        .iter()
        .enumerate()
        .filter(|(_, bit)| mask & **bit != 0)
        .map(|(i, _)| format!("L{}", i + 1))
        .collect();
    if names.is_empty() {
        "none".to_string()
    } else {
        names.join("+")
    }
}

/// One summary line: requested and active levers, plus the HRESULT of each lever that failed.
pub fn lever_report(h: &S2Header) -> String {
    let mut t = format!(
        "levers requested {}, active {}",
        lever_names(h.levers_requested),
        lever_names(h.levers_active)
    );
    for (i, hr) in h.lever_hr.iter().enumerate() {
        if *hr != 0 {
            let _ = write!(t, ", failed L{} hr 0x{:08X}", i + 1, *hr as u32);
        }
    }
    t
}

#[cfg(test)]
#[path = "levers_tests.rs"]
mod tests;
