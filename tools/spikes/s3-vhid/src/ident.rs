//! Telling our HID collections apart from other devices.
//! The device is root-enumerated, so collection paths read `HID#HIDCLASS&ColNN#...` and carry no
//! VID/PID: identity comes from `HidD_GetAttributes`, the collection number from the path.

pub const VHID_VID: u16 = 0xDEED;
pub const VHID_PID: u16 = 0xFEED;

pub fn is_vhid(vid: u16, pid: u16) -> bool {
    vid == VHID_VID && pid == VHID_PID
}

/// `&ColNN` in a HID device path (`&Col05` -> 5); None for single-collection devices.
pub fn collection_from_path(path: &str) -> Option<u8> {
    let upper = path.to_ascii_uppercase();
    let at = upper.find("&COL")?;
    upper.get(at + 4..at + 6)?.parse().ok()
}

#[cfg(test)]
#[path = "ident_tests.rs"]
mod tests;
