use super::*;

#[test]
fn collection_from_root_enumerated_path() {
    let path = r"\\?\HID#HIDCLASS&Col05#1&2d595ca7&0&0004#{4d1e55b2-f16f-11cf-88cb-001111000030}";
    assert_eq!(collection_from_path(path), Some(5));
    let path = r"\\?\HID#HIDCLASS&Col01#1&2d595ca7&0&0000#{884b96c3-56ef-11d1-bc8c-00a0c91405dd}";
    assert_eq!(collection_from_path(path), Some(1));
}

#[test]
fn collection_from_vid_pid_path() {
    let path =
        r"\\?\HID#VID_DEED&PID_FEED&Col04#7&1a2b3c&0&0003#{4d1e55b2-f16f-11cf-88cb-001111000030}";
    assert_eq!(collection_from_path(path), Some(4));
}

#[test]
fn no_collection_in_single_collection_path() {
    let path =
        r"\\?\HID#VID_046D&PID_C52B&MI_00#8&2f3c&0&0000#{884b96c3-56ef-11d1-bc8c-00a0c91405dd}";
    assert_eq!(collection_from_path(path), None);
}

#[test]
fn vhid_is_matched_by_attributes_not_path() {
    assert!(is_vhid(0xDEED, 0xFEED));
    assert!(!is_vhid(0xDEED, 0x0001));
    assert!(!is_vhid(0x046D, 0xFEED));
}
