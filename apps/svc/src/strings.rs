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

#[cfg(test)]
#[path = "strings_tests.rs"]
mod tests;
