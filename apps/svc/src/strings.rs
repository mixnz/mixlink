//! Text a person reads: the Services list entry and the install / uninstall messages
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
