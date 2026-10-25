use windows::Win32::Devices::DeviceAndDriverInstallation::PNP_VetoOutstandingOpen;

use super::*;

#[test]
fn multi_sz_double_nul() {
    let id = r"Root\MixLinkVHid";
    let w = multi_sz(id);
    assert_eq!(w.len(), id.len() + 2);
    assert_eq!(&w[w.len() - 2..], &[0, 0]);
    assert_eq!(String::from_utf16_lossy(&w[..id.len()]), id);
}

#[test]
fn hardware_ids_match_case_insensitive() {
    let mut list: Vec<u16> = "USB\\Other".encode_utf16().chain([0]).collect();
    list.extend(multi_sz(r"root\mixlinkvhid"));
    assert!(hardware_ids_match(&list, r"Root\MixLinkVHid"));
    assert!(!hardware_ids_match(&list, r"Root\MixLinkVHidX"));
    assert!(!hardware_ids_match(&list, r"Root\MixLink"));
}

#[test]
fn hardware_ids_match_empty_list() {
    assert!(!hardware_ids_match(&[], r"Root\MixLinkVHid"));
    assert!(!hardware_ids_match(&[0, 0], r"Root\MixLinkVHid"));
}

#[test]
fn veto_app_only_for_windows_app() {
    let name: Vec<u16> = "Steam".encode_utf16().chain([0, 0, 0]).collect();
    assert_eq!(
        veto_app(PNP_VetoWindowsApp, &name),
        Some("Steam".to_owned())
    );
    assert_eq!(veto_app(PNP_VetoOutstandingOpen, &name), None);
    assert_eq!(veto_app(PNP_VetoWindowsApp, &[0, 0]), None);
}
