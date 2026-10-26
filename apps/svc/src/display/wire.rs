//! Byte layout of the `vdisplay` control IOCTLs (spec 0015), mirrored in `drivers/vdisplay/Control.h`.
//! Every field is a little-endian u32 (the LUID high part is an i32); every request starts with
//! `version` and `flags` (bit 0 = HDR, reserved: kept 0 until the HDR work).

/// `CTL_CODE(FILE_DEVICE_UNKNOWN, 0x900, METHOD_BUFFERED, FILE_ANY_ACCESS)`.
pub const IOCTL_ADD_MONITOR: u32 = 0x0022_2400;
pub const IOCTL_REMOVE_MONITOR: u32 = 0x0022_2404;
pub const IOCTL_QUERY: u32 = 0x0022_2408;

pub const WIRE_VERSION: u32 = 1;
pub const MAX_MODES: usize = 16;
pub const MAX_EDID: usize = 512;
pub const MAX_MONITORS: usize = 4;
const EDID_BLOCK: usize = 128;
const MODE_SIZE: usize = 12;
const MONITOR_SIZE: usize = 20;

pub const ADD_SIZE: usize = 740;
pub const REMOVE_SIZE: usize = 12;
pub const QUERY_IN_SIZE: usize = 8;
pub const QUERY_SIZE: usize = 104;

const ADD_EDID_AT: usize = 36;
const ADD_MODES_AT: usize = ADD_EDID_AT + MAX_EDID;
const QUERY_MONITORS_AT: usize = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WireMode {
    pub width: u32,
    pub height: u32,
    pub refresh_mhz: u32,
}

/// `ADD_MONITOR` input: container GUID bytes, EDID blocks, modes (`preferred` indexes them).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddMonitor {
    pub container: [u8; 16],
    pub edid: Vec<u8>,
    pub modes: Vec<WireMode>,
    pub preferred: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MonitorInfo {
    pub id: u32,
    pub connector: u32,
    pub mode: WireMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryReply {
    pub iddcx_version: u32,
    /// LUID as (low part, high part); valid for this boot only, never persisted.
    pub render_luid: (u32, i32),
    pub monitors: Vec<MonitorInfo>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum WireError {
    #[error("a monitor needs at least one mode")]
    NoModes,
    #[error("a monitor takes at most {MAX_MODES} modes")]
    TooManyModes,
    #[error("EDID must be 1 to 4 blocks of 128 bytes")]
    EdidLength,
    #[error("preferred mode index is out of range")]
    BadPreferred,
    #[error("driver reply is too short")]
    Short,
    #[error("driver speaks another control version")]
    Version,
    #[error("driver reply lists too many monitors")]
    Count,
}

fn put(buf: &mut [u8], at: usize, value: u32) {
    buf[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn get(buf: &[u8], at: usize) -> u32 {
    let mut bytes = [0u8; 4];
    bytes.copy_from_slice(&buf[at..at + 4]);
    u32::from_le_bytes(bytes)
}

fn header(buf: &mut [u8]) {
    put(buf, 0, WIRE_VERSION);
    put(buf, 4, 0);
}

fn len_u32(len: usize) -> u32 {
    u32::try_from(len).unwrap_or(u32::MAX)
}

fn put_mode(buf: &mut [u8], at: usize, mode: &WireMode) {
    put(buf, at, mode.width);
    put(buf, at + 4, mode.height);
    put(buf, at + 8, mode.refresh_mhz);
}

fn get_mode(buf: &[u8], at: usize) -> WireMode {
    WireMode {
        width: get(buf, at),
        height: get(buf, at + 4),
        refresh_mhz: get(buf, at + 8),
    }
}

impl AddMonitor {
    pub fn encode(&self) -> Result<[u8; ADD_SIZE], WireError> {
        if self.modes.is_empty() {
            return Err(WireError::NoModes);
        }
        if self.modes.len() > MAX_MODES {
            return Err(WireError::TooManyModes);
        }
        if self.edid.is_empty()
            || self.edid.len() > MAX_EDID
            || !self.edid.len().is_multiple_of(EDID_BLOCK)
        {
            return Err(WireError::EdidLength);
        }
        if usize::try_from(self.preferred).map_or(true, |i| i >= self.modes.len()) {
            return Err(WireError::BadPreferred);
        }
        let mut buf = [0u8; ADD_SIZE];
        header(&mut buf);
        buf[8..24].copy_from_slice(&self.container);
        put(&mut buf, 24, len_u32(self.edid.len()));
        put(&mut buf, 28, len_u32(self.modes.len()));
        put(&mut buf, 32, self.preferred);
        buf[ADD_EDID_AT..ADD_EDID_AT + self.edid.len()].copy_from_slice(&self.edid);
        for (i, mode) in self.modes.iter().enumerate() {
            put_mode(&mut buf, ADD_MODES_AT + i * MODE_SIZE, mode);
        }
        Ok(buf)
    }
}

pub fn encode_remove(id: u32) -> [u8; REMOVE_SIZE] {
    let mut buf = [0u8; REMOVE_SIZE];
    header(&mut buf);
    put(&mut buf, 8, id);
    buf
}

pub fn encode_query() -> [u8; QUERY_IN_SIZE] {
    let mut buf = [0u8; QUERY_IN_SIZE];
    header(&mut buf);
    buf
}

/// `ADD_MONITOR` output: the new monitor's id.
pub fn decode_add_reply(buf: &[u8]) -> Result<u32, WireError> {
    if buf.len() < 4 {
        return Err(WireError::Short);
    }
    Ok(get(buf, 0))
}

pub fn decode_query(buf: &[u8]) -> Result<QueryReply, WireError> {
    if buf.len() < QUERY_SIZE {
        return Err(WireError::Short);
    }
    if get(buf, 0) != WIRE_VERSION {
        return Err(WireError::Version);
    }
    let count = usize::try_from(get(buf, 20)).map_err(|_| WireError::Count)?;
    if count > MAX_MONITORS {
        return Err(WireError::Count);
    }
    let monitors = (0..count)
        .map(|i| {
            let at = QUERY_MONITORS_AT + i * MONITOR_SIZE;
            MonitorInfo {
                id: get(buf, at),
                connector: get(buf, at + 4),
                mode: get_mode(buf, at + 8),
            }
        })
        .collect();
    Ok(QueryReply {
        iddcx_version: get(buf, 8),
        render_luid: (get(buf, 12), get(buf, 16).cast_signed()),
        monitors,
    })
}

#[cfg(test)]
#[path = "wire_tests.rs"]
mod tests;
