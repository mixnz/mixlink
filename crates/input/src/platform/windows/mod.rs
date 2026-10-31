//! Windows-only code: the `vhid` driver feeder (spec 0016), the client keyboard hook and thread wake
//! (spec 0021). All `unsafe` of the crate lives here.

pub mod hook_thread;
pub mod kbd_hook;
pub mod raw_keyboard;
pub mod thread_msg;
pub mod vhid;
pub mod vhid_wire;
