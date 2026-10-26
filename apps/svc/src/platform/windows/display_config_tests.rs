use super::*;
use windows::Win32::Devices::Display::{
    DISPLAYCONFIG_PATH_TARGET_INFO, DISPLAYCONFIG_SCANLINE_ORDERING_INTERLACED,
    DISPLAYCONFIG_SCANLINE_ORDERING_PROGRESSIVE, DISPLAYCONFIG_SCANLINE_ORDERING_UNSPECIFIED,
};

fn name(path: &str, product: u16) -> TargetName {
    TargetName {
        monitor_path: path.to_owned(),
        edid_manufacture_id: MXL_MFG_ID,
        edid_product_code_id: product,
    }
}

#[test]
fn mixlink_target_needs_a_device_path() {
    assert!(is_ready_mixlink(
        &name(r"\?\DISPLAY#MXL4D58#1", 0x4D58),
        0x4D58
    ));
    assert!(!is_ready_mixlink(&name("", 0x4D58), 0x4D58));
    assert!(!is_ready_mixlink(
        &name(r"\?\DISPLAY#MXL4D59#1", 0x4D59),
        0x4D58
    ));
    let mut other = name(r"\?\DISPLAY#DEL1234#1", 0x4D58);
    other.edid_manufacture_id = 0xAC10;
    assert!(!is_ready_mixlink(&other, 0x4D58));
}

#[test]
fn inactive_path_gets_progressive_scanline() {
    let mut target = DISPLAYCONFIG_PATH_TARGET_INFO {
        scanLineOrdering: DISPLAYCONFIG_SCANLINE_ORDERING_UNSPECIFIED,
        ..Default::default()
    };
    set_target_refresh(&mut target, 160, 1);
    assert_eq!(
        target.scanLineOrdering,
        DISPLAYCONFIG_SCANLINE_ORDERING_PROGRESSIVE
    );
    assert_eq!(
        (target.refreshRate.Numerator, target.refreshRate.Denominator),
        (160, 1)
    );
}

#[test]
fn known_scanline_is_kept() {
    let mut target = DISPLAYCONFIG_PATH_TARGET_INFO {
        scanLineOrdering: DISPLAYCONFIG_SCANLINE_ORDERING_INTERLACED,
        ..Default::default()
    };
    set_target_refresh(&mut target, 60_000, 1000);
    assert_eq!(
        target.scanLineOrdering,
        DISPLAYCONFIG_SCANLINE_ORDERING_INTERLACED
    );
}

#[test]
fn phantom_target_is_not_a_display() {
    // With no monitor plugged in, Windows keeps one path active with no monitor device path.
    let mut phantom = name("", 0);
    phantom.edid_manufacture_id = 0;
    assert!(!counts_as_display(&phantom, false));
    let mut msi = name(r"\?\DISPLAY#MSI3CF0#5&361e361&0&UID4355", 0x3CF0);
    msi.edid_manufacture_id = 0x6936;
    assert!(counts_as_display(&msi, false));
    let mixlink = name(r"\?\DISPLAY#MXL4D59#1", 0x4D59);
    assert!(!counts_as_display(&mixlink, false));
    assert!(counts_as_display(&mixlink, true));
}
