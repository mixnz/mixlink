//! Lane helpers shared by `serve` and `run`.

use std::fmt;
use std::io::{self, Write};
use std::net::SocketAddrV4;

use mixlink_core::ids::FrameId;
use mixlink_net::TxLane;
use mixlink_proto::header::{Flags, PacketHeader};
use mixlink_proto::packetizer::Packetizer;

/// Writes one line to stdout (the probe's report); a closed stdout is ignored.
pub fn say(line: fmt::Arguments<'_>) {
    let _ = writeln!(io::stdout().lock(), "{line}");
}

/// Queues every fragment of `message`; `false` if the lane ran out of slots (message lost).
pub fn send_message(
    tx: &mut TxLane,
    packetizer: &mut Packetizer,
    frame: FrameId,
    message: &[u8],
    to: SocketAddrV4,
) -> bool {
    let Ok(mut fragments) = packetizer.packetize(frame, Flags::empty(), message) else {
        return false;
    };
    for _ in 0..fragments.count() {
        let Some(mut slot) = tx.try_reserve() else {
            return false;
        };
        let Some(len) = fragments.next_into(slot.datagram()) else {
            break;
        };
        slot.commit(len, to);
    }
    true
}

/// Header + payload of a received datagram, `None` if it is not a valid v1 packet.
pub fn decode(datagram: &[u8]) -> Option<(PacketHeader, &[u8])> {
    PacketHeader::decode(datagram).ok()
}
