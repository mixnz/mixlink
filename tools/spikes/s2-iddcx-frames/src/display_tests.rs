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

#[test]
fn check_passes_when_virtual_is_main_at_120() {
    assert_eq!(
        check_verdict(r"\.\DISPLAY3", 120, Some(r"\.\DISPLAY3")),
        Ok(r"main \.\DISPLAY3 120 Hz, virtual \.\DISPLAY3 is main".to_string())
    );
}

#[test]
fn check_fails_when_virtual_not_main() {
    let err = check_verdict(r"\.\DISPLAY1", 120, Some(r"\.\DISPLAY3")).unwrap_err();
    assert_eq!(
        err,
        r"main \.\DISPLAY1 120 Hz, virtual \.\DISPLAY3 is not main: make the virtual monitor the main display"
    );
}

#[test]
fn check_fails_on_wrong_refresh_or_no_virtual() {
    assert!(check_verdict(r"\.\DISPLAY3", 60, Some(r"\.\DISPLAY3")).is_err());
    assert!(check_verdict(r"\.\DISPLAY1", 120, None).is_err());
}
