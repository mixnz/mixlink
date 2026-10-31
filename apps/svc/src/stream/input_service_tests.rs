use std::net::SocketAddrV4;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self as std_mpsc, RecvTimeoutError};
use std::time::Duration;

use mixlink_core::ids::FrameId;
use mixlink_input::client::state::InputState;
use mixlink_input::hid::report::Report;
use mixlink_input::host::abs_map::AbsMap;
use mixlink_net::{Transport, TransportConfig, TxLane};
use mixlink_proto::channel::Channel;
use mixlink_proto::crypto::{Role, Sealer};
use mixlink_proto::header::Flags;
use mixlink_proto::input::{INPUT_LEN, encode};
use mixlink_proto::packetizer::Packetizer;

use super::*;
use crate::stream::video_send::tests::keys;

/// Reports go back to the test.
struct ChanSink(std_mpsc::Sender<Report>);

impl ReportSink for ChanSink {
    type Error = String;

    fn feed(&mut self, report: &Report) -> Result<(), String> {
        self.0.send(*report).map_err(|e| e.to_string())
    }
}

struct Client {
    tx: TxLane,
    sealer: Sealer,
    packetizer: Packetizer,
    frame: FrameId,
    host: SocketAddrV4,
}

impl Client {
    fn send(&mut self, state: &mut InputState) {
        let mut body = [0u8; INPUT_LEN];
        encode(&state.packet(), &mut body);
        let mut fragments = self
            .packetizer
            .packetize(self.frame, Flags::empty(), &body)
            .unwrap();
        self.frame = self.frame.next();
        let mut slot = self.tx.try_reserve().unwrap();
        let len = fragments.next_into(slot.datagram()).unwrap();
        slot.seal_commit(&mut self.sealer, len, self.host).unwrap();
    }
}

/// A lane wakes only the first thread that waited on it: one thread serves every session, and the
/// second session's keys are applied at once, not at the next 50 ms wait.
#[test]
fn second_session_is_woken_at_once() {
    let (host, mut host_lanes) = Transport::start(TransportConfig::loopback()).unwrap();
    let (client, mut client_lanes) = Transport::start(TransportConfig::loopback()).unwrap();
    let mut rx = host_lanes.take_rx(Channel::Input).unwrap();
    let mut c = Client {
        tx: client_lanes.take_tx(Channel::Input).unwrap(),
        sealer: keys().sealer(Role::Client, Channel::Input),
        packetizer: Packetizer::new(Channel::Input),
        frame: FrameId::new(0),
        host: host.local_addr(),
    };
    let (reports_tx, reports) = std_mpsc::channel();
    let (commands, inbox) = channel();
    let stop = Arc::new(AtomicBool::new(false));
    let stopper = Arc::clone(&stop);
    let worker = std::thread::spawn(move || {
        let mut sink = ChanSink(reports_tx);
        run(&mut rx, &mut sink, &inbox, &|| {
            stopper.load(Ordering::Acquire)
        });
    });
    let control = InputControl::new(commands, worker.thread().clone());
    let session = || InputRecv::new(&keys(), client.local_addr(), AbsMap::new((1920, 1080)));
    for round in 0..2 {
        control.start(session());
        std::thread::sleep(Duration::from_millis(80));
        let mut state = InputState::new();
        state.key(0x04, true);
        c.send(&mut state);
        let first = reports.recv_timeout(Duration::from_millis(20));
        assert!(first.is_ok(), "round {round}: key not applied within 20 ms");
        control.stop();
        // The stop releases the key: drain what it fed.
        while !matches!(
            reports.recv_timeout(Duration::from_millis(100)),
            Err(RecvTimeoutError::Timeout)
        ) {}
    }
    stop.store(true, Ordering::Release);
    worker.thread().unpark();
    worker.join().unwrap();
}
