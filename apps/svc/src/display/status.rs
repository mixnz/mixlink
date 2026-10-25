//! Text for `display status` (spec 0015).

use std::fmt::Write;

use super::wire::QueryReply;

/// Refresh in Hz without trailing zeros: 60000 mHz → "60", 59940 → "59.94".
pub fn refresh_text(refresh_mhz: u32) -> String {
    let (whole, fraction) = (refresh_mhz / 1000, refresh_mhz % 1000);
    if fraction == 0 {
        return whole.to_string();
    }
    let digits = format!("{fraction:03}");
    format!("{whole}.{}", digits.trim_end_matches('0'))
}

pub fn format_status(reply: &QueryReply, gpu_name: Option<&str>, saved_layout: bool) -> String {
    let mut out = String::new();
    if reply.monitors.is_empty() {
        out.push_str("MixLink monitors: none\n");
    } else {
        let _ = writeln!(out, "MixLink monitors: {}", reply.monitors.len());
        for monitor in &reply.monitors {
            let _ = writeln!(
                out,
                "  #{}  connector {}  {}x{} at {} Hz",
                monitor.id,
                monitor.connector,
                monitor.mode.width,
                monitor.mode.height,
                refresh_text(monitor.mode.refresh_mhz)
            );
        }
    }
    let (low, high) = reply.render_luid;
    let _ = writeln!(
        out,
        "Render GPU: {} (LUID {:08x}:{low:08x})",
        gpu_name.unwrap_or("unknown"),
        high.cast_unsigned()
    );
    let _ = writeln!(out, "IddCx version: {:#x}", reply.iddcx_version);
    let _ = writeln!(
        out,
        "Saved screen layout: {}",
        if saved_layout { "yes" } else { "no" }
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::display::wire::{MonitorInfo, WireMode};

    fn reply(monitors: Vec<MonitorInfo>) -> QueryReply {
        QueryReply {
            iddcx_version: 0x1A00,
            render_luid: (0xD2F1, 0),
            monitors,
        }
    }

    #[test]
    fn status_lists_monitors_and_gpu() {
        let text = format_status(
            &reply(vec![
                MonitorInfo {
                    id: 1,
                    connector: 0,
                    mode: WireMode {
                        width: 2560,
                        height: 1440,
                        refresh_mhz: 120_000,
                    },
                },
                MonitorInfo {
                    id: 3,
                    connector: 2,
                    mode: WireMode {
                        width: 1920,
                        height: 1080,
                        refresh_mhz: 59_940,
                    },
                },
            ]),
            Some("NVIDIA GeForce RTX 4080"),
            true,
        );
        assert!(text.contains("MixLink monitors: 2\n"));
        assert!(text.contains("  #1  connector 0  2560x1440 at 120 Hz\n"));
        assert!(text.contains("  #3  connector 2  1920x1080 at 59.94 Hz\n"));
        assert!(text.contains("Render GPU: NVIDIA GeForce RTX 4080 (LUID 00000000:0000d2f1)\n"));
        assert!(text.contains("IddCx version: 0x1a00\n"));
        assert!(text.contains("Saved screen layout: yes\n"));
    }

    #[test]
    fn status_without_monitors_or_gpu_name() {
        let text = format_status(&reply(Vec::new()), None, false);
        assert!(text.contains("MixLink monitors: none\n"));
        assert!(text.contains("Render GPU: unknown (LUID 00000000:0000d2f1)\n"));
        assert!(text.contains("Saved screen layout: no\n"));
    }

    #[test]
    fn refresh_text_trims_zeros() {
        assert_eq!(refresh_text(60_000), "60");
        assert_eq!(refresh_text(59_940), "59.94");
        assert_eq!(refresh_text(143_856), "143.856");
        assert_eq!(refresh_text(74_500), "74.5");
    }
}
