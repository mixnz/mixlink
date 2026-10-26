//! Identity of a virtual Xbox pad node per slot (spec 0016).
//!
//! hidclass derives the HID child's hardware ID from the node's hardware ID (`Root\X` → `HID\X`), so
//! the node carries `Root\VID_045E&PID_0B13&IG_00`: the child then matches the inbox `xinputhid.inf`
//! (`HID\VID_045E&PID_0B13&IG_00`) and Windows binds the XInput filter. The instance ID only has to be
//! unique per slot.

pub use crate::args::PAD_SLOTS;

/// Hardware ID that selects the pad install section of `vhid.inf`. hidclass derives the HID child's
/// hardware ID from it (`HID\VID_045E&PID_0B13&IG_00`), which is what `xinputhid.inf` matches.
pub const PAD_HARDWARE_ID: &str = r"Root\VID_045E&PID_0B13&IG_00";

/// Container GUID base `{4d4c5844-5041-4400-0000-00000000000N}`; never the null container, which makes
/// XInput skip a slot.
const CONTAINER_BASE: u128 = 0x4d4c_5844_5041_4400_0000_0000_0000_0000;

pub fn valid_slot(slot: u8) -> bool {
    slot < PAD_SLOTS
}

/// Full device instance ID of the node for `slot`, e.g. `ROOT\VID_045E&PID_0B13&IG_00\0000`.
pub fn pad_instance_id(slot: u8) -> String {
    format!(r"ROOT\VID_045E&PID_0B13&IG_00\{slot:04}")
}

pub fn pad_container_guid(slot: u8) -> u128 {
    CONTAINER_BASE | u128::from(slot)
}

#[cfg(test)]
#[path = "pad_ids_tests.rs"]
mod tests;
