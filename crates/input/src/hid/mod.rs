//! HID report builders for `drivers/vhid` (spec 0016): pure, allocation-free, layouts mirrored in
//! `drivers/vhid/Descriptor.c`.

pub mod keyboard;
pub mod mouse;
pub mod report;
pub mod xbox;

#[cfg(test)]
#[path = "hid_tests.rs"]
mod tests;
