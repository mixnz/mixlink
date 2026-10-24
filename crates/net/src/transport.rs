//! `Transport`: one UDP socket, a send thread and a recv thread, and per-channel lanes.

use std::array;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread::{self, JoinHandle};

use mixlink_core::config::NetConfig;
use mixlink_proto::channel::Channel;

use crate::error::NetError;
use crate::lane::{self, Reader, RxLane, TxLane, Wake, Writer};
use crate::recv_loop::RecvLoop;
use crate::send_loop::{PRIORITY, SendLoop};
use crate::socket::{self, READ_TIMEOUT, SOCKET_BUFFER, SocketConfig};
use crate::stats::{LaneCounters, SocketCounters, TransportStats, lane_index};

/// Slots per lane, indexed by `channel.id() - 1`; the same capacity is used in both directions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LaneCapacities([usize; 6]);

impl Default for LaneCapacities {
    fn default() -> Self {
        // Order of `Channel::ALL`: Control, Video, Audio, Cursor, Input, Feedback.
        Self([128, 4096, 64, 512, 256, 64])
    }
}

impl LaneCapacities {
    pub fn get(&self, channel: Channel) -> usize {
        self.0[lane_index(channel)]
    }

    pub fn set(&mut self, channel: Channel, slots: usize) {
        self.0[lane_index(channel)] = slots;
    }
}

#[derive(Debug, Clone)]
pub struct TransportConfig {
    pub socket: SocketConfig,
    pub lanes: LaneCapacities,
}

impl TransportConfig {
    /// Host: listens on `0.0.0.0:session_port`.
    pub fn host(net: &NetConfig) -> Self {
        Self::bound(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, net.session_port))
    }

    /// Client: any interface, ephemeral port.
    pub fn client() -> Self {
        Self::bound(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0))
    }

    /// `127.0.0.1`, ephemeral port: tests and local tools.
    pub fn loopback() -> Self {
        Self::bound(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0))
    }

    fn bound(bind: SocketAddrV4) -> Self {
        Self {
            socket: SocketConfig {
                bind,
                buffer_bytes: SOCKET_BUFFER,
                read_timeout: READ_TIMEOUT,
            },
            lanes: LaneCapacities::default(),
        }
    }
}

/// Application ends of every lane; each is taken once and moved to the thread that owns it.
#[derive(Debug)]
pub struct Lanes {
    tx: [Option<TxLane>; 6],
    rx: [Option<RxLane>; 6],
}

impl Lanes {
    pub fn take_tx(&mut self, channel: Channel) -> Option<TxLane> {
        self.tx[lane_index(channel)].take()
    }

    pub fn take_rx(&mut self, channel: Channel) -> Option<RxLane> {
        self.rx[lane_index(channel)].take()
    }
}

#[derive(Debug)]
pub struct Transport {
    local: SocketAddrV4,
    stop: Arc<AtomicBool>,
    send_wake: Arc<Wake>,
    threads: Vec<JoinHandle<()>>,
    tx_counters: [Arc<LaneCounters>; 6],
    rx_counters: [Arc<LaneCounters>; 6],
    socket_counters: Arc<SocketCounters>,
    recv_buffer: usize,
    send_buffer: usize,
    mmcss_send: bool,
    mmcss_recv: bool,
}

impl Transport {
    /// Opens the socket, allocates every slot and starts `mixlink-net-send` / `mixlink-net-recv`.
    pub fn start(config: TransportConfig) -> Result<(Transport, Lanes), NetError> {
        let open = socket::open(&config.socket)?;
        let local = match open.socket.local_addr() {
            Ok(SocketAddr::V4(addr)) => addr,
            _ => config.socket.bind,
        };
        let socket = Arc::new(open.socket);
        let stop = Arc::new(AtomicBool::new(false));
        let send_wake = Arc::new(Wake::default());
        let tx_counters: [Arc<LaneCounters>; 6] = array::from_fn(|_| Arc::default());
        let rx_counters: [Arc<LaneCounters>; 6] = array::from_fn(|_| Arc::default());
        let socket_counters = Arc::new(SocketCounters::default());

        let mut tx_app: [Option<TxLane>; 6] = Default::default();
        let mut tx_lane = |channel: Channel| -> Reader {
            let i = lane_index(channel);
            let (writer, reader) = lane::pair(
                config.lanes.get(channel),
                Arc::clone(&send_wake),
                Arc::clone(&tx_counters[i]),
            );
            tx_app[i] = Some(TxLane(writer));
            reader
        };
        let send_lanes = PRIORITY.map(&mut tx_lane);
        let video = tx_lane(Channel::Video);

        let mut rx_app: [Option<RxLane>; 6] = Default::default();
        let recv_lanes: [Writer; 6] = array::from_fn(|i| {
            let channel = Channel::ALL[i];
            let (writer, reader) = lane::pair(
                config.lanes.get(channel),
                Arc::new(Wake::default()),
                Arc::clone(&rx_counters[lane_index(channel)]),
            );
            rx_app[lane_index(channel)] = Some(RxLane(reader));
            writer
        });

        let mut transport = Transport {
            local,
            stop: Arc::clone(&stop),
            send_wake: Arc::clone(&send_wake),
            threads: Vec::with_capacity(2),
            tx_counters,
            rx_counters,
            socket_counters: Arc::clone(&socket_counters),
            recv_buffer: open.recv_buffer,
            send_buffer: open.send_buffer,
            mmcss_send: false,
            mmcss_recv: false,
        };
        let send = SendLoop {
            socket: Arc::clone(&socket),
            lanes: send_lanes,
            video,
            wake: send_wake,
            stop: Arc::clone(&stop),
            counters: Arc::clone(&socket_counters),
        };
        let (send_ready_tx, send_ready_rx) = mpsc::channel();
        transport.spawn("mixlink-net-send", move || send.run(send_ready_tx))?;
        let recv = RecvLoop {
            socket,
            lanes: recv_lanes,
            stop,
            counters: socket_counters,
        };
        let (recv_ready_tx, recv_ready_rx) = mpsc::channel();
        transport.spawn("mixlink-net-recv", move || recv.run(recv_ready_tx))?;
        transport.mmcss_send = send_ready_rx.recv().unwrap_or(false);
        transport.mmcss_recv = recv_ready_rx.recv().unwrap_or(false);
        let lanes = Lanes {
            tx: tx_app,
            rx: rx_app,
        };
        Ok((transport, lanes))
    }

    /// Spawns a named thread; on failure the threads already running are stopped by `Drop`.
    fn spawn(&mut self, name: &str, body: impl FnOnce() + Send + 'static) -> Result<(), NetError> {
        let handle = thread::Builder::new()
            .name(name.into())
            .spawn(body)
            .map_err(NetError::Spawn)?;
        self.threads.push(handle);
        Ok(())
    }

    pub fn local_addr(&self) -> SocketAddrV4 {
        self.local
    }

    pub fn stats(&self) -> TransportStats {
        let c = &self.socket_counters;
        TransportStats {
            tx: array::from_fn(|i| self.tx_counters[i].snapshot()),
            rx: array::from_fn(|i| self.rx_counters[i].snapshot()),
            send_err: c.send_err.load(Ordering::Relaxed),
            recv_err: c.recv_err.load(Ordering::Relaxed),
            rx_bad: c.rx_bad.load(Ordering::Relaxed),
            recv_buffer: self.recv_buffer,
            send_buffer: self.send_buffer,
            mmcss_send: self.mmcss_send,
            mmcss_recv: self.mmcss_recv,
        }
    }

    /// Stops and joins both threads; the recv thread notices within the read timeout.
    /// Idempotent; also run on drop.
    pub fn shutdown(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.send_wake.wake();
        for handle in self.threads.drain(..) {
            if handle.join().is_err() {
                log::error!("network thread panicked");
            }
        }
    }
}

impl Drop for Transport {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
#[path = "transport_tests.rs"]
mod tests;
