//! Virtual display control (spec 0015): driver wire format, EDID, modes.

pub mod cvt;
pub mod edid;
pub mod modes;
pub mod status;
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "used by the display session flow next (spec 0015)"
    )
)]
pub mod store;
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "used by the display session flow next (spec 0015)"
    )
)]
pub mod topology;

#[cfg(windows)]
pub mod commands;
pub mod ports;
pub mod wire;
