//! Text a person reads: the Services list entry, the install / uninstall and driver messages
//! (ADR-0003: translated with the language task).

pub const SERVICE_DISPLAY_NAME: &str = "MixLink";
pub const SERVICE_DESCRIPTION: &str =
    "Streams this PC to MixLink clients on your network. Stopping it ends any stream.";

pub const INSTALLED: &str = "MixLink service installed and running.";
pub const UPDATED: &str = "MixLink service updated and restarted.";
pub const REMOVED: &str = "MixLink service removed.";
pub const NOT_INSTALLED: &str = "MixLink service isn't installed. Nothing to remove.";
pub const FIREWALL_ADDED: &str =
    "Firewall rule added, so MixLink clients on a private network can find this PC.";
pub const FIREWALL_REMOVED: &str = "Firewall rule removed.";
/// Shown in Windows Defender Firewall next to the rule.
pub const FIREWALL_RULE_DESCRIPTION: &str =
    "Lets MixLink clients on your network find this PC and stream from it.";
pub const NOT_BY_SCM: &str = "This program runs as a Windows service. Set it up with \
     `mixlink-svc install`, or run it here with `mixlink-svc --console`.";

pub const DRIVERS_INSTALLED: &str = "MixLink drivers installed: virtual display and virtual input.";
pub const DRIVERS_UPDATED: &str = "MixLink drivers updated.";
pub const DRIVERS_REMOVED: &str = "MixLink drivers removed.";
pub const DRIVERS_NOT_INSTALLED: &str = "MixLink drivers aren't installed. Nothing to remove.";

/// Uninstall stopped because a program holds a MixLink device open (spec 0014).
pub fn drivers_held(app: Option<&str>) -> String {
    match app {
        Some(app) => format!("{app} is using a MixLink device. Close it, then run this again."),
        None => "A program is using the MixLink virtual controller, usually Steam. \
                 Close it, then run this again."
            .to_owned(),
    }
}

fn with_inf(text: &str, inf: Option<&str>) -> String {
    match inf {
        Some(inf) => format!("{text} ({inf})"),
        None => text.to_owned(),
    }
}

pub const STATUS_CERT_LABEL: &str = "Certificate: ";

pub fn status_not_installed() -> String {
    "not installed".to_owned()
}

pub fn status_package_missing(inf: &str) -> String {
    format!("package {inf} is missing from the driver store. Run `mixlink-svc drivers install`.")
}

pub fn status_no_device(inf: Option<&str>) -> String {
    with_inf("package installed, no device", inf)
}

pub fn status_not_started(inf: Option<&str>) -> String {
    with_inf("installed but not started", inf)
}

pub fn status_working(inf: Option<&str>) -> String {
    with_inf("working", inf)
}

pub fn status_problem(code: u32, inf: Option<&str>) -> String {
    with_inf(&format!("Device Manager problem code {code}"), inf)
}

pub fn status_cert_trusted(thumbprint: &str) -> String {
    format!("trusted ({thumbprint})")
}

pub fn status_cert_untrusted(thumbprint: &str) -> String {
    format!("{thumbprint} is not trusted any more. Run `mixlink-svc drivers install`.")
}

pub fn status_cert_stray() -> String {
    "a MixLink certificate is trusted but not recorded. Run `mixlink-svc drivers uninstall`."
        .to_owned()
}

pub fn status_cert_none() -> String {
    "none".to_owned()
}

/// `drivers install | uninstall` stopped the service and could not start it again (spec 0015).
pub fn service_not_restarted(error: &str) -> String {
    format!("MixLink service didn't start again ({error}). Start it with `sc start MixLink`.")
}

/// `display try`: the monitor is up (spec 0015).
pub fn monitor_on(width: u32, height: u32, refresh: &str, took_ms: u128) -> String {
    format!(
        "MixLink monitor is on: {width}x{height} at {refresh} Hz (ready in {took_ms} ms). \
         Press Enter to remove it."
    )
}

pub const MONITOR_OFF: &str = "MixLink monitor removed.";

/// `input try`: seconds left before the script starts (spec 0016).
pub fn input_try_countdown(seconds: u32) -> String {
    format!("Starting in {seconds} s. Switch to the window you want to test.")
}

pub const INPUT_TRY_DONE: &str = "Done. Every key and button is released.";
pub const INPUT_TRY_HOLD: &str =
    "F13 is held down. Press Ctrl+C or end this process to check that it gets released.";

/// `input pad` start-up help (spec 0016).
pub fn input_pad_help(controls: &str) -> String {
    format!(
        "Type a control and press Enter to press it for 0.5 s, or add a time in ms (lt 2000).\n\
         Controls: {controls}\n\
         prime: move every trigger and stick once. q: quit."
    )
}

pub fn input_pad_pressed(hold_ms: u32) -> String {
    format!("Pressed for {hold_ms} ms.")
}

pub fn input_pad_unknown(text: &str, controls: &str) -> String {
    format!("Unknown control \"{text}\". Controls: {controls}")
}

pub const INPUT_PAD_PRIMED: &str = "Moved every trigger and stick once.";

/// `input pad-add` / `pad-remove` (spec 0016). Slots are shown 1-based, like player numbers.
pub fn pad_added(slot: u8) -> String {
    format!("Virtual Xbox controller {} added.", slot + 1)
}

pub fn pad_replaced(slot: u8) -> String {
    format!(
        "Virtual Xbox controller {} added again (an old one was left over).",
        slot + 1
    )
}

pub fn pad_removed(slot: u8) -> String {
    format!("Virtual Xbox controller {} removed.", slot + 1)
}

pub fn pad_absent(slot: u8) -> String {
    format!(
        "There's no virtual Xbox controller {}. Nothing to remove.",
        slot + 1
    )
}

/// `input probe` result line (spec 0016).
pub fn probe_summary(
    device: &str,
    p50_us: f64,
    p99_us: f64,
    max_us: f64,
    sent: usize,
    received: usize,
    lost: usize,
) -> String {
    format!(
        "{device}: p50 {:.2} ms, p99 {:.2} ms, max {:.2} ms; sent {sent}, received {received}, lost {lost}",
        p50_us / 1000.0,
        p99_us / 1000.0,
        max_us / 1000.0
    )
}

/// `capture try`: recording started (spec 0019).
pub fn capture_started(cmd: &crate::capture_args::CaptureTry) -> String {
    let codec = match cmd.codec {
        mixlink_codec::VideoCodec::H264 => "H.264",
        mixlink_codec::VideoCodec::Hevc => "HEVC",
    };
    format!(
        "Recording the MixLink monitor ({}x{} at {} Hz, {codec}) to {} for {} s. \
         Frames only come when the picture changes, so keep something moving on it. Ctrl+C stops early.",
        cmd.mode.width,
        cmd.mode.height,
        crate::display::status::refresh_text(cmd.mode.refresh_mhz),
        cmd.out.display(),
        cmd.seconds
    )
}

/// `capture try`: the numbers at the end (spec 0019).
pub fn capture_report(stats: &crate::capture::report::Stats, out: &std::path::Path) -> String {
    let ms = |ns: u64| ns as f64 / 1_000_000.0;
    let kib = |bytes: u64| bytes / 1024;
    let (cap, enc, au) = (&stats.capture, &stats.encode, &stats.au_bytes);
    format!(
        "Saved {}. Frames {}, keyframes {}, size changes {}.\n\
         Capture (present to encoder): p50 {:.2} ms, p99 {:.2} ms, max {:.2} ms\n\
         Encode: p50 {:.2} ms, p99 {:.2} ms, max {:.2} ms\n\
         Frame size: p50 {} KiB, max {} KiB\n\
         Driver waits that ran out: {}, ring copies: {}, frames not saved (a keyframe follows each): {}",
        out.display(),
        stats.frames,
        stats.idrs,
        stats.reopens.saturating_sub(1),
        ms(cap.percentile(50.0)),
        ms(cap.percentile(99.0)),
        ms(cap.max()),
        ms(enc.percentile(50.0)),
        ms(enc.percentile(99.0)),
        ms(enc.max()),
        kib(au.percentile(50.0)),
        kib(au.max()),
        stats.hold_timeouts,
        stats.ring_fallbacks,
        stats.dropped_aus
    )
}

#[cfg(test)]
#[path = "strings_tests.rs"]
mod tests;
