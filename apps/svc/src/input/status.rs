//! Text for `input status` (spec 0016).

use std::fmt::Write;

use mixlink_input::platform::windows::vhid_wire::QueryReply;

pub fn format_status(reply: &QueryReply) -> String {
    let [keyboard, rel, abs, pad] = reply.sizes;
    let mut out = String::new();
    let _ = writeln!(
        out,
        "MixLink virtual input: wire version {}; keyboard {keyboard} B, mouse {rel} B / {abs} B, gamepad {pad} B",
        reply.version
    );
    let _ = writeln!(out, "Dropped reports: {}", reply.dropped);
    let feeder = if reply.feeder_held {
        "in use (the MixLink service or `input try`)"
    } else {
        "free"
    };
    let _ = writeln!(out, "Feeder: {feeder}");
    out
}

#[cfg(test)]
#[path = "status_tests.rs"]
mod tests;
