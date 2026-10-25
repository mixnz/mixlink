//! `drivers status`: what is installed, as plain text for people (spec 0014 → Status).

use crate::strings;

/// A device node of one of our drivers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceState {
    /// No node with our hardware ID.
    Missing,
    /// A node exists but is not started (e.g. not present after a failed install).
    NotStarted,
    /// Started; `0` = working, else the Device Manager problem code.
    Problem(u32),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DriverStatus {
    pub name: &'static str,
    /// Published INF name from the saved state.
    pub published: Option<String>,
    /// The published INF is in the driver store.
    pub in_store: bool,
    pub device: DeviceState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusReport {
    pub drivers: Vec<DriverStatus>,
    /// Thumbprint from the saved state.
    pub cert: Option<String>,
    /// A certificate with our subject is in the trust stores (the saved one, when `cert` is set).
    pub cert_trusted: bool,
}

/// One line per driver, then one for the certificate.
pub fn format_status(report: &StatusReport) -> String {
    let mut out = String::new();
    for driver in &report.drivers {
        let state = match (&driver.published, driver.in_store, driver.device) {
            (None, _, DeviceState::Missing) => strings::status_not_installed(),
            (Some(inf), false, _) => strings::status_package_missing(inf),
            (inf, _, DeviceState::Missing) => strings::status_no_device(inf.as_deref()),
            (inf, _, DeviceState::NotStarted) => strings::status_not_started(inf.as_deref()),
            (inf, _, DeviceState::Problem(0)) => strings::status_working(inf.as_deref()),
            (inf, _, DeviceState::Problem(code)) => strings::status_problem(code, inf.as_deref()),
        };
        out.push_str(&format!("{}: {state}\n", driver.name));
    }
    let cert = match (&report.cert, report.cert_trusted) {
        (Some(thumb), true) => strings::status_cert_trusted(thumb),
        (Some(thumb), false) => strings::status_cert_untrusted(thumb),
        (None, true) => strings::status_cert_stray(),
        (None, false) => strings::status_cert_none(),
    };
    out.push_str(&format!("{}{cert}\n", strings::STATUS_CERT_LABEL));
    out
}

#[cfg(test)]
#[path = "status_tests.rs"]
mod tests;
