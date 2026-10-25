use super::*;

const THUMB: &str = "0123456789ABCDEF0123456789ABCDEF01234567";

fn driver(
    name: &'static str,
    published: Option<&str>,
    in_store: bool,
    device: DeviceState,
) -> DriverStatus {
    DriverStatus {
        name,
        published: published.map(str::to_owned),
        in_store,
        device,
    }
}

fn report(drivers: Vec<DriverStatus>, cert: Option<&str>, cert_trusted: bool) -> StatusReport {
    StatusReport {
        drivers,
        cert: cert.map(str::to_owned),
        cert_trusted,
    }
}

#[test]
fn not_installed() {
    let text = format_status(&report(
        vec![
            driver("vdisplay", None, false, DeviceState::Missing),
            driver("vhid", None, false, DeviceState::Missing),
        ],
        None,
        false,
    ));
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 3, "{text}");
    assert!(
        lines[0].starts_with("vdisplay: ") && lines[0].contains("not installed"),
        "{text}"
    );
    assert!(
        lines[1].starts_with("vhid: ") && lines[1].contains("not installed"),
        "{text}"
    );
    assert!(lines[2].starts_with("Certificate: "), "{text}");
}

#[test]
fn installed_ok() {
    let text = format_status(&report(
        vec![driver(
            "vhid",
            Some("oem11.inf"),
            true,
            DeviceState::Problem(0),
        )],
        Some(THUMB),
        true,
    ));
    assert!(text.contains("vhid: working"), "{text}");
    assert!(text.contains("oem11.inf"), "{text}");
    assert!(text.contains(THUMB), "{text}");
    assert!(text.contains("trusted"), "{text}");
}

#[test]
fn device_problem_shown() {
    let text = format_status(&report(
        vec![driver(
            "vdisplay",
            Some("oem10.inf"),
            true,
            DeviceState::Problem(28),
        )],
        Some(THUMB),
        true,
    ));
    assert!(text.contains("28"), "{text}");
    assert!(!text.contains("working"), "{text}");
}

#[test]
fn not_started_shown() {
    let text = format_status(&report(
        vec![driver(
            "vdisplay",
            Some("oem10.inf"),
            true,
            DeviceState::NotStarted,
        )],
        Some(THUMB),
        true,
    ));
    assert!(text.contains("not started"), "{text}");
}

#[test]
fn package_missing_from_store_flagged() {
    let text = format_status(&report(
        vec![driver(
            "vhid",
            Some("oem11.inf"),
            false,
            DeviceState::Missing,
        )],
        Some(THUMB),
        true,
    ));
    assert!(
        text.contains("oem11.inf") && text.contains("missing"),
        "{text}"
    );
}

#[test]
fn cert_missing_flagged() {
    let text = format_status(&report(Vec::new(), Some(THUMB), false));
    assert!(text.contains("not trusted"), "{text}");
    assert!(text.contains("drivers install"), "{text}");
}

#[test]
fn stray_cert_flagged() {
    let text = format_status(&report(Vec::new(), None, true));
    assert!(text.contains("drivers uninstall"), "{text}");
}
