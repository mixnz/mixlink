//! Client keyboard and mouse to the host (spec 0021): the sealed sender, the repeat schedule, the window
//! shortcuts, and (Windows) the `stream-input` thread with the keyboard hook.

pub mod capture;
pub mod sender;
pub mod shortcuts;
#[cfg(windows)]
pub mod thread;
