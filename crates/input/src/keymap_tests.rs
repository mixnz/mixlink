use super::*;
use crate::hid::keyboard::KeyboardState;

const VK_OTHER: u16 = 0x41;

#[test]
fn letters_and_digits() {
    assert_eq!(usage(0x1E, false, VK_OTHER), Some(0x04));
    assert_eq!(usage(0x2C, false, VK_OTHER), Some(0x1D));
    assert_eq!(usage(0x02, false, VK_OTHER), Some(0x1E));
    assert_eq!(usage(0x0B, false, VK_OTHER), Some(0x27));
}

#[test]
fn extended_keys() {
    assert_eq!(usage(0x1D, true, VK_OTHER), Some(0xE4));
    assert_eq!(usage(0x38, true, VK_OTHER), Some(0xE6));
    assert_eq!(usage(0x5B, true, VK_OTHER), Some(0xE3));
    assert_eq!(usage(0x48, true, VK_OTHER), Some(0x52));
    assert_eq!(usage(0x37, true, VK_OTHER), Some(0x46));
    assert_eq!(usage(0x1C, true, VK_OTHER), Some(0x58));
}

#[test]
fn same_scan_differs_with_extended() {
    assert_eq!(usage(0x1D, false, VK_OTHER), Some(0xE0));
    assert_eq!(usage(0x48, false, VK_OTHER), Some(0x60));
    assert_eq!(usage(0x37, false, VK_OTHER), Some(0x55));
}

#[test]
fn pause_and_numlock_by_vk() {
    assert_eq!(usage(0x45, false, 0x13), Some(0x48));
    assert_eq!(usage(0x45, true, 0x90), Some(0x53));
    assert_eq!(usage(0x45, false, 0x90), Some(0x53));
}

#[test]
fn altgr_fake_ctrl_is_dropped() {
    assert_eq!(usage(0x21D, false, 0xA2), None);
}

#[test]
fn unknown_scan_is_none() {
    assert_eq!(usage(0x00, false, VK_OTHER), None);
    assert_eq!(usage(0x20, true, VK_OTHER), None);
}

#[test]
fn every_usage_fits_the_report() {
    for scan in 0..=0x7Fu16 {
        for extended in [false, true] {
            if let Some(u) = usage(scan, extended, VK_OTHER) {
                assert!(
                    KeyboardState::default().set(u, true),
                    "scan {scan:#x} ext {extended} -> {u:#x}"
                );
            }
        }
    }
}
