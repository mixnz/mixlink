//! Cursor send (spec 0020): shape on change (fragmented), position per update (one packet), sealed on
//! the Cursor lane. Message ids +1 per message, so the client's reassembler drops stale ones.

use std::net::SocketAddrV4;

use mixlink_capture::cursor_header::{CursorPos, ShapeInfo, ShapeType};
use mixlink_core::ids::FrameId;
use mixlink_net::TxLane;
use mixlink_proto::channel::Channel;
use mixlink_proto::crypto::{Role, Sealer, SessionKeys};
use mixlink_proto::cursor::{
    CURSOR_POSITION_LEN, CURSOR_SHAPE_HEADER_LEN, CursorPosition, CursorShapeHeader, ShapeKind,
};
use mixlink_proto::header::Flags;
use mixlink_proto::packetizer::Packetizer;

/// 256 x 256 BGRA + the shape header.
const SHAPE_MESSAGE_MAX: usize = CURSOR_SHAPE_HEADER_LEN + 256 * 256 * 4;

#[derive(Debug)]
pub struct CursorSender {
    tx: TxLane,
    sealer: Sealer,
    peer: SocketAddrV4,
    packetizer: Packetizer,
    buf: Box<[u8]>,
    message: FrameId,
}

impl CursorSender {
    pub fn new(tx: TxLane, keys: &SessionKeys, peer: SocketAddrV4) -> CursorSender {
        CursorSender {
            tx,
            sealer: keys.sealer(Role::Host, Channel::Cursor),
            peer,
            packetizer: Packetizer::new(Channel::Cursor),
            buf: vec![0u8; SHAPE_MESSAGE_MAX].into_boxed_slice(),
            message: FrameId::new(0),
        }
    }

    /// `false` when it did not fit the lane or the buffer (the client asks again for a missing shape).
    pub fn send_shape(&mut self, info: &ShapeInfo, shape_id: u32, pixels: &[u8]) -> bool {
        let header = CursorShapeHeader {
            shape_id,
            kind: match info.kind {
                ShapeType::Alpha => ShapeKind::Color,
                ShapeType::MaskedColor => ShapeKind::MaskedColor,
            },
            width: info.width,
            height: info.height,
            hot_x: info.hot_x,
            hot_y: info.hot_y,
            pitch: info.pitch,
        };
        match header.encode_into(pixels, &mut self.buf) {
            Ok(len) => self.queue(len),
            Err(error) => {
                log::debug!("cursor shape {shape_id} not sent: {error}");
                false
            }
        }
    }

    // HOT PATH
    pub fn send_position(&mut self, pos: &CursorPos) -> bool {
        let position = CursorPosition {
            x: pos.x,
            y: pos.y,
            visible: pos.visible,
            shape_id: pos.shape_id,
        };
        let Some(out) = self.buf.first_chunk_mut::<CURSOR_POSITION_LEN>() else {
            return false;
        };
        position.encode(out);
        self.queue(CURSOR_POSITION_LEN)
    }

    fn queue(&mut self, len: usize) -> bool {
        let message = self.message;
        self.message = message.next();
        let Ok(mut fragments) =
            self.packetizer
                .packetize(message, Flags::empty(), &self.buf[..len])
        else {
            return false;
        };
        for _ in 0..fragments.count() {
            let Some(mut slot) = self.tx.try_reserve() else {
                return false;
            };
            let Some(datagram_len) = fragments.next_into(slot.datagram()) else {
                break;
            };
            if slot
                .seal_commit(&mut self.sealer, datagram_len, self.peer)
                .is_err()
            {
                return false;
            }
        }
        true
    }

    pub fn into_lane(self) -> TxLane {
        self.tx
    }
}

/// Pixels of a shape by mask byte and colour, logged on each new shape: the masked-colour meaning
/// was settled from these (spec 0020 Results).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ShapeCensus {
    pub full_black: u32,
    pub full_colour: u32,
    pub zero_black: u32,
    pub zero_colour: u32,
    pub other: u32,
}

pub fn census(pixels: &[u8], pitch: u32, width: u16, height: u16) -> ShapeCensus {
    let mut c = ShapeCensus::default();
    let row_bytes = usize::from(width) * 4;
    for row in pixels
        .chunks(pitch.max(1) as usize)
        .take(usize::from(height))
    {
        let Some(row) = row.get(..row_bytes) else {
            break;
        };
        for px in row.as_chunks::<4>().0 {
            let black = px[..3] == [0, 0, 0];
            let count = match (px[3], black) {
                (0xFF, true) => &mut c.full_black,
                (0xFF, false) => &mut c.full_colour,
                (0, true) => &mut c.zero_black,
                (0, false) => &mut c.zero_colour,
                _ => &mut c.other,
            };
            *count += 1;
        }
    }
    c
}

#[cfg(test)]
#[path = "cursor_send_tests.rs"]
mod tests;
