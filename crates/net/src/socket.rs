//! Session socket setup: IPv4 UDP, large buffers, read timeout, Windows UDP fixes.

use std::net::{SocketAddrV4, UdpSocket};
use std::time::Duration;

use socket2::{Domain, Protocol, SockAddr, Socket, Type};

use crate::error::NetError;
use crate::platform;

/// Send and receive buffer size: one keyframe burst (~4.3 MB on the wire) must fit.
pub const SOCKET_BUFFER: usize = 4 * 1024 * 1024;
/// The recv thread checks its stop flag at least this often.
pub const READ_TIMEOUT: Duration = Duration::from_millis(20);

#[derive(Debug, Clone)]
pub struct SocketConfig {
    pub bind: SocketAddrV4,
    pub buffer_bytes: usize,
    pub read_timeout: Duration,
}

/// An open socket plus the buffer sizes Windows actually applied.
#[derive(Debug)]
pub struct OpenSocket {
    pub socket: UdpSocket,
    pub recv_buffer: usize,
    pub send_buffer: usize,
}

fn option(option: &'static str) -> impl FnOnce(std::io::Error) -> NetError {
    move |source| NetError::SocketOption { option, source }
}

pub fn open(config: &SocketConfig) -> Result<OpenSocket, NetError> {
    let socket =
        Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP)).map_err(option("socket"))?;
    socket
        .set_recv_buffer_size(config.buffer_bytes)
        .map_err(option("SO_RCVBUF"))?;
    socket
        .set_send_buffer_size(config.buffer_bytes)
        .map_err(option("SO_SNDBUF"))?;
    socket
        .bind(&SockAddr::from(config.bind))
        .map_err(|source| NetError::Bind {
            addr: config.bind,
            source,
        })?;
    socket
        .set_read_timeout(Some(config.read_timeout))
        .map_err(option("SO_RCVTIMEO"))?;
    let recv_buffer = socket.recv_buffer_size().map_err(option("SO_RCVBUF"))?;
    let send_buffer = socket.send_buffer_size().map_err(option("SO_SNDBUF"))?;
    let socket: UdpSocket = socket.into();
    platform::disable_udp_connreset(&socket).map_err(option("SIO_UDP_CONNRESET"))?;
    Ok(OpenSocket {
        socket,
        recv_buffer,
        send_buffer,
    })
}

#[cfg(test)]
#[path = "socket_tests.rs"]
mod tests;
