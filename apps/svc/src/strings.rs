//! Text a person reads: the Services list entry, the install / uninstall and driver messages
//! (ADR-0003: translated with the language task).

pub const SERVICE_DISPLAY_NAME: &str = "MixLink";
pub const SERVICE_DESCRIPTION: &str =
    "Streams this PC to MixLink clients on your network. Stopping it ends any stream.";

pub const INSTALLED: &str = "MixLink service installed and running.";
pub const UPDATED: &str = "MixLink service updated and restarted.";
pub const REMOVED: &str = "MixLink service removed.";
pub const NOT_INSTALLED: &str = "MixLink service isn't installed. Nothing to remove.";
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

/// `display try`: the monitor is up (spec 0015).
pub fn monitor_on(width: u32, height: u32, refresh: &str) -> String {
    format!("MixLink monitor is on: {width}x{height} at {refresh} Hz. Press Enter to remove it.")
}

pub const MONITOR_OFF: &str = "MixLink monitor removed.";

#[cfg(test)]
#[path = "strings_tests.rs"]
mod tests;
