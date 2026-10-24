//! Recv thread: reads datagrams and routes them to the rx lane of their channel.

use std::io::ErrorKind;
use std::net::{SocketAddr, SocketAddrV4, UdpSocket};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;

use mixlink_core::clock;
use mixlink_proto::channel::Channel;
use mixlink_proto::header::{HEADER_LEN, WIRE_VERSION};

use crate::lane::{SLOT_LEN, Writer};
use crate::platform::Mmcss;
use crate::stats::{SocketCounters, lane_index};

/// Staging buffer: bigger than any valid datagram, so oversize ones are seen and dropped.
const STAGING_LEN: usize = 1500;
/// `WSAEMSGSIZE`: the datagram did not fit the staging buffer.
const WSAEMSGSIZE: i32 = 10040;

pub(crate) struct RecvLoop {
    pub(crate) socket: Arc<UdpSocket>,
    /// Indexed by `lane_index`.
    pub(crate) lanes: [Writer; 6],
    pub(crate) stop: Arc<AtomicBool>,
    pub(crate) counters: Arc<SocketCounters>,
}

impl RecvLoop {
    /// Thread body; reports MMCSS on `ready`, then reads until `stop` (seen within the read timeout).
    pub(crate) fn run(mut self, ready: Sender<bool>) {
        let mmcss = Mmcss::enter();
        let _ = ready.send(mmcss.is_some());
        let mut staging = [0u8; STAGING_LEN];
        while !self.stop.load(Ordering::Acquire) {
            self.receive(&mut staging);
        }
        drop(mmcss);
    }

    // HOT PATH
    fn receive(&mut self, staging: &mut [u8; STAGING_LEN]) {
        match self.socket.recv_from(staging) {
            Ok((len, SocketAddr::V4(peer))) => self.route(&staging[..len], peer),
            Ok((_, SocketAddr::V6(_))) => SocketCounters::bump(&self.counters.rx_bad),
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
            Err(e) if e.raw_os_error() == Some(WSAEMSGSIZE) => {
                SocketCounters::bump(&self.counters.rx_bad);
            }
            Err(_) => SocketCounters::bump(&self.counters.recv_err),
        }
    }

    // HOT PATH
    /// Routes on bytes 0-1 only; the consumer runs the full `PacketHeader::decode`.
    fn route(&mut self, datagram: &[u8], peer: SocketAddrV4) {
        let channel = match datagram {
            [WIRE_VERSION, id, ..] if (HEADER_LEN..=SLOT_LEN).contains(&datagram.len()) => {
                Channel::from_id(*id)
            }
            _ => None,
        };
        let Some(channel) = channel else {
            SocketCounters::bump(&self.counters.rx_bad);
            return;
        };
        let Some(lane) = self.lanes.get_mut(lane_index(channel)) else {
            SocketCounters::bump(&self.counters.rx_bad);
            return;
        };
        // A full lane is counted by the lane itself.
        if let Some(mut slot) = lane.try_reserve() {
            slot.buf()[..datagram.len()].copy_from_slice(datagram);
            slot.commit(datagram.len(), peer, clock::now());
        }
    }
}
