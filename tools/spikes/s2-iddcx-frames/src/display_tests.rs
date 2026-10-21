use super::*;

#[test]
fn adapter_path_matches_root_display() {
    assert!(is_virtual_adapter_path(
        r"\\?\ROOT#DISPLAY#0000#{5b45201d-f2f2-4f3b-85bb-30ff1f953599}"
    ));
    assert!(is_virtual_adapter_path(r"\\?\root#display#0000#{x}"));
}

#[test]
fn adapter_path_rejects_gpu() {
    assert!(!is_virtual_adapter_path(
        r"\\?\PCI#VEN_10DE&DEV_2684#4&1#{5b45201d-f2f2-4f3b-85bb-30ff1f953599}"
    ));
}

#[test]
fn wide_stops_at_nul() {
    let mut w = [0u16; 8];
    for (i, c) in "AB".encode_utf16().enumerate() {
        w[i] = c;
    }
    assert_eq!(from_wide(&w), "AB");
}
