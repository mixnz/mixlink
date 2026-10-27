use std::time::Duration;

use mixlink_net::{RxLane, Transport, TransportConfig};
use mixlink_proto::crypto::Opener;
use mixlink_proto::cursor::{self, CursorMessage};
use mixlink_proto::header::PacketHeader;
use mixlink_proto::reassembler::{Push, Reassembler, ReassemblerConfig};

use super::*;
use crate::stream::video_send::tests::keys;

struct Pair {
    _host: Transport,
    _client: Transport,
    rx: RxLane,
    opener: Opener,
    datagrams: usize,
    reassembler: Reassembler,
    sender: CursorSender,
}

fn pair() -> Pair {
    let (host, mut host_lanes) = Transport::start(TransportConfig::loopback()).unwrap();
    let (client, mut client_lanes) = Transport::start(TransportConfig::loopback()).unwrap();
    let sender = CursorSender::new(
        host_lanes.take_tx(Channel::Cursor).unwrap(),
        &keys(),
        client.local_addr(),
    );
    Pair {
        _host: host,
        rx: client_lanes.take_rx(Channel::Cursor).unwrap(),
        _client: client,
        opener: keys().opener(Role::Client, Channel::Cursor),
        datagrams: 0,
        reassembler: Reassembler::new(ReassemblerConfig::for_channel(Channel::Cursor)),
        sender,
    }
}

/// Next cursor message, decoded into an owned form: (position, or shape header + pixel count).
fn receive(p: &mut Pair) -> Option<Result<CursorPosition, (CursorShapeHeader, usize)>> {
    while let Some(mut dg) = p.rx.recv_timeout(Duration::from_millis(500)) {
        p.datagrams += 1;
        let plain = dg.open(&mut p.opener).unwrap();
        let (header, payload) = PacketHeader::decode(plain).unwrap();
        if let Push::Complete(msg) = p.reassembler.push(&header, payload) {
            return Some(match cursor::decode(msg.data).unwrap() {
                CursorMessage::Position(pos) => Ok(pos),
                CursorMessage::Shape { header, pixels } => Err((header, pixels.len())),
            });
        }
    }
    None
}

fn info() -> ShapeInfo {
    ShapeInfo {
        kind: ShapeType::MaskedColor,
        width: 32,
        height: 32,
        hot_x: 2,
        hot_y: 3,
        pitch: 128,
        len: 128 * 32,
    }
}

#[test]
fn shape_then_position() {
    let mut p = pair();
    let pixels = vec![0xAB; 128 * 32];
    assert!(p.sender.send_shape(&info(), 41, &pixels));
    let pos = CursorPos {
        x: -5,
        y: 700,
        visible: true,
        shape_id: 41,
    };
    assert!(p.sender.send_position(&pos));
    let (header, len) = receive(&mut p).unwrap().unwrap_err();
    assert_eq!(header.shape_id, 41);
    assert_eq!(header.kind, ShapeKind::MaskedColor);
    assert_eq!((header.width, header.hot_x, header.hot_y), (32, 2, 3));
    assert_eq!(len, 128 * 32);
    assert_eq!(
        receive(&mut p).unwrap().unwrap(),
        CursorPosition {
            x: -5,
            y: 700,
            visible: true,
            shape_id: 41
        }
    );
}

#[test]
fn position_is_one_datagram() {
    let mut p = pair();
    let pos = CursorPos {
        x: 1,
        y: 2,
        visible: false,
        shape_id: 0,
    };
    assert!(p.sender.send_position(&pos));
    assert!(receive(&mut p).unwrap().is_ok());
    assert_eq!(p.datagrams, 1);
}

#[test]
fn alpha_maps_to_color() {
    let mut p = pair();
    let mut alpha = info();
    alpha.kind = ShapeType::Alpha;
    assert!(p.sender.send_shape(&alpha, 1, &vec![0; 128 * 32]));
    let (header, _) = receive(&mut p).unwrap().unwrap_err();
    assert_eq!(header.kind, ShapeKind::Color);
}

#[test]
fn bad_pixel_length_is_not_sent() {
    let mut p = pair();
    assert!(!p.sender.send_shape(&info(), 1, &[0; 10]));
}
