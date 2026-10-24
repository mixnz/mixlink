//! Probe-only payloads (input / feedback bodies are not defined yet).
//! Byte 0 is the kind: 1 ping `{seq: u32, sent: u64 ticks}`, 2 frame ack `{frame: u32}`; LE.

const PING: u8 = 1;
const ACK: u8 = 2;
pub const PING_LEN: usize = 13;
pub const ACK_LEN: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Probe {
    /// Client → host on `Input`; the host echoes the same bytes on `Feedback`.
    Ping { seq: u32, sent: u64 },
    /// Host → client on `Feedback`: video frame `frame` reassembled.
    Ack { frame: u32 },
}

impl Probe {
    /// Writes into `out` (at least 13 bytes) and returns the length; 0 if `out` is too short.
    pub fn encode(&self, out: &mut [u8]) -> usize {
        match *self {
            Probe::Ping { seq, sent } => {
                let Some(b) = out.first_chunk_mut::<PING_LEN>() else {
                    return 0;
                };
                b[0] = PING;
                b[1..5].copy_from_slice(&seq.to_le_bytes());
                b[5..13].copy_from_slice(&sent.to_le_bytes());
                PING_LEN
            }
            Probe::Ack { frame } => {
                let Some(b) = out.first_chunk_mut::<ACK_LEN>() else {
                    return 0;
                };
                b[0] = ACK;
                b[1..5].copy_from_slice(&frame.to_le_bytes());
                ACK_LEN
            }
        }
    }

    pub fn decode(bytes: &[u8]) -> Option<Probe> {
        match bytes {
            [PING, rest @ ..] => {
                let (seq, rest) = rest.split_first_chunk::<4>()?;
                let sent = rest.first_chunk::<8>()?;
                Some(Probe::Ping {
                    seq: u32::from_le_bytes(*seq),
                    sent: u64::from_le_bytes(*sent),
                })
            }
            [ACK, rest @ ..] => Some(Probe::Ack {
                frame: u32::from_le_bytes(*rest.first_chunk::<4>()?),
            }),
            _ => None,
        }
    }
}

#[cfg(test)]
#[path = "wire_tests.rs"]
mod tests;
