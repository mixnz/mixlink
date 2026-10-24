use std::io::ErrorKind;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};
use std::time::{Duration, Instant};

use super::*;
use crate::error::NetError;

fn loopback(port: u16) -> SocketConfig {
    SocketConfig {
        bind: SocketAddrV4::new(Ipv4Addr::LOCALHOST, port),
        buffer_bytes: SOCKET_BUFFER,
        read_timeout: READ_TIMEOUT,
    }
}

fn port_of(socket: &UdpSocket) -> u16 {
    socket.local_addr().map(|a| a.port()).unwrap_or(0)
}

#[test]
fn binds_ephemeral_port() {
    let open = open(&loopback(0)).unwrap();
    assert_ne!(port_of(&open.socket), 0);
}

#[test]
fn applies_buffer_sizes() {
    let open = open(&loopback(0)).unwrap();
    assert!(
        open.recv_buffer >= SOCKET_BUFFER,
        "recv {}",
        open.recv_buffer
    );
    assert!(
        open.send_buffer >= SOCKET_BUFFER,
        "send {}",
        open.send_buffer
    );
}

#[test]
fn bind_conflict_is_bind_error() {
    let first = open(&loopback(0)).unwrap();
    let port = port_of(&first.socket);
    match open(&loopback(port)) {
        Err(NetError::Bind { addr, .. }) => {
            assert_eq!(addr, SocketAddrV4::new(Ipv4Addr::LOCALHOST, port));
        }
        other => panic!("expected Bind error, got {other:?}"),
    }
}

#[test]
fn read_times_out() {
    let open = open(&loopback(0)).unwrap();
    let mut buf = [0u8; 64];
    let start = Instant::now();
    let err = open.socket.recv_from(&mut buf).unwrap_err();
    assert!(
        matches!(err.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut),
        "{err:?}"
    );
    assert!(start.elapsed() < Duration::from_millis(200));
}

#[cfg(windows)]
#[test]
fn closed_port_does_not_break_recv() {
    let a = open(&loopback(0)).unwrap();
    let b = open(&loopback(0)).unwrap();
    let released = {
        let probe = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        port_of(&probe)
    };
    a.socket
        .send_to(&[1, 2, 3, 4], (Ipv4Addr::LOCALHOST, released))
        .unwrap();
    std::thread::sleep(Duration::from_millis(50));
    let a_addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port_of(&a.socket)));
    b.socket.send_to(&[9, 8, 7], a_addr).unwrap();
    let mut buf = [0u8; 64];
    let (len, _) = a.socket.recv_from(&mut buf).unwrap();
    assert_eq!(&buf[..len], &[9, 8, 7]);
}
