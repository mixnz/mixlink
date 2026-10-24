//! `SIO_UDP_CONNRESET` off: otherwise one ICMP "port unreachable" (peer closed) makes the next
//! `recv_from` fail with `WSAECONNRESET`.

use std::io;
use std::net::UdpSocket;
use std::os::windows::io::AsRawSocket;

use windows::Win32::Networking::WinSock::{SIO_UDP_CONNRESET, SOCKET, WSAIoctl};

pub(crate) fn disable_udp_connreset(socket: &UdpSocket) -> io::Result<()> {
    let enable: u32 = 0;
    let mut returned: u32 = 0;
    let raw = SOCKET(socket.as_raw_socket() as usize);
    // SAFETY: `raw` is a live socket owned by `socket`; the input buffer is a 4-byte BOOL that
    // outlives the synchronous call; no output buffer, no overlapped I/O.
    let result = unsafe {
        WSAIoctl(
            raw,
            SIO_UDP_CONNRESET,
            Some(std::ptr::from_ref(&enable).cast()),
            4,
            None,
            0,
            &mut returned,
            None,
            None,
        )
    };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}
