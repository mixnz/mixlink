//! Text for `input status` (spec 0016).

use std::fmt::Write;

use mixlink_input::hid::xbox::PAD_SLOTS;
use mixlink_input::platform::windows::vhid_wire::QueryReply;

pub fn format_status(reply: &QueryReply) -> String {
    let [keyboard, rel, abs, pad] = reply.sizes;
    let mut out = String::new();
    let _ = writeln!(
        out,
        "MixLink virtual input: wire version {}; keyboard {keyboard} B, mouse {rel} B / {abs} B, Xbox pad {pad} B",
        reply.version
    );
    let _ = writeln!(out, "Dropped reports: {}", reply.dropped);
    let feeder = if reply.feeder_held {
        "in use (the MixLink service or `input try`)"
    } else {
        "free"
    };
    let _ = writeln!(out, "Feeder: {feeder}");
    // Slots are shown 1-based, like player numbers.
    let pads: Vec<String> = (0..PAD_SLOTS)
        .filter(|slot| reply.pads_present & (1 << slot) != 0)
        .map(|slot| (slot + 1).to_string())
        .collect();
    let pads = if pads.is_empty() {
        "none".to_owned()
    } else {
        pads.join(", ")
    };
    let _ = writeln!(out, "Virtual Xbox controllers: {pads}");
    out
}

#[cfg(test)]
#[path = "status_tests.rs"]
mod tests;
