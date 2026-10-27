//! `net-probe serve`: answers `Hello`, echoes pings, acks every reassembled video frame.

use std::error::Error;
use std::net::SocketAddrV4;
use std::thread;
use std::time::Duration;

use mixlink_core::config::NetConfig;
use mixlink_core::ids::{FrameId, SessionId};
use mixlink_net::{RxLane, Transport, TransportConfig, TxLane};
use mixlink_proto::channel::Channel;
use mixlink_proto::control::{self, ControlMessage};
use mixlink_proto::packetizer::Packetizer;
use mixlink_proto::reassembler::{Push, Reassembler, ReassemblerConfig};

use crate::link::{decode, say, send_message};
use crate::wire::{ACK_LEN, Probe};

struct Server {
    transport: Transport,
    control_rx: RxLane,
    input_rx: RxLane,
    video_rx: RxLane,
    control_tx: TxLane,
    feedback_tx: TxLane,
    control_out: Packetizer,
    feedback_out: Packetizer,
    out_frame: FrameId,
    control_in: Reassembler,
    video_in: Reassembler,
    peer: Option<SocketAddrV4>,
    pings: u64,
    acks: u64,
}

pub fn serve(port: u16) -> Result<(), Box<dyn Error>> {
    let net = NetConfig {
        session_port: port,
        ..NetConfig::default()
    };
    let (transport, mut lanes) = Transport::start(TransportConfig::host(&net))?;
    let lane_missing = "lane already taken";
    let mut server = Server {
        control_rx: lanes.take_rx(Channel::Control).ok_or(lane_missing)?,
        input_rx: lanes.take_rx(Channel::Input).ok_or(lane_missing)?,
        video_rx: lanes.take_rx(Channel::Video).ok_or(lane_missing)?,
        control_tx: lanes.take_tx(Channel::Control).ok_or(lane_missing)?,
        feedback_tx: lanes.take_tx(Channel::Feedback).ok_or(lane_missing)?,
        control_out: Packetizer::new(Channel::Control),
        feedback_out: Packetizer::new(Channel::Feedback),
        out_frame: FrameId::new(0),
        control_in: Reassembler::new(ReassemblerConfig::for_channel(Channel::Control)),
        video_in: Reassembler::new(ReassemblerConfig::for_channel(Channel::Video)),
        peer: None,
        pings: 0,
        acks: 0,
        transport,
    };
    server.control_rx.attach_current_thread();
    server.input_rx.attach_current_thread();
    server.video_rx.attach_current_thread();
    say(format_args!(
        "net-probe serve: listening on {} (Ctrl+C to stop)",
        server.transport.local_addr()
    ));
    loop {
        let busy = server.control() | server.input() | server.video();
        if !busy {
            thread::park_timeout(Duration::from_millis(5));
        }
    }
}

impl Server {
    fn next_frame(&mut self) -> FrameId {
        self.out_frame = self.out_frame.next();
        self.out_frame
    }

    fn control(&mut self) -> bool {
        let mut busy = false;
        while let Some(datagram) = self.control_rx.try_recv() {
            busy = true;
            let from = datagram.peer();
            let Some((header, payload)) = decode(datagram.data()) else {
                continue;
            };
            let Push::Complete(message) = self.control_in.push(&header, payload) else {
                continue;
            };
            match control::decode(message.data) {
                Ok(ControlMessage::Hello {
                    displays,
                    stream_display,
                    name,
                    ..
                }) => {
                    let Some(&mode) = displays.get(usize::from(stream_display)) else {
                        continue;
                    };
                    let Ok(welcome) = control::encode(&ControlMessage::Welcome {
                        session: SessionId::new(1),
                        mode,
                        codec: mixlink_proto::control::CodecId::H264,
                    }) else {
                        continue;
                    };
                    let frame = self.next_frame();
                    send_message(
                        &mut self.control_tx,
                        &mut self.control_out,
                        frame,
                        &welcome,
                        from,
                    );
                    if self.peer != Some(from) {
                        say(format_args!("client {from} ({name}) connected"));
                        self.peer = Some(from);
                        self.video_in =
                            Reassembler::new(ReassemblerConfig::for_channel(Channel::Video));
                        self.pings = 0;
                        self.acks = 0;
                    }
                }
                Ok(ControlMessage::Bye { .. }) if self.peer == Some(from) => {
                    self.report(from);
                    self.peer = None;
                }
                _ => {}
            }
        }
        busy
    }

    fn input(&mut self) -> bool {
        let mut busy = false;
        while let Some(datagram) = self.input_rx.try_recv() {
            busy = true;
            if self.peer != Some(datagram.peer()) {
                continue;
            }
            let Some((_, payload)) = decode(datagram.data()) else {
                continue;
            };
            if let Some(Probe::Ping { .. }) = Probe::decode(payload) {
                self.out_frame = self.out_frame.next();
                let frame = self.out_frame;
                let to = datagram.peer();
                send_message(
                    &mut self.feedback_tx,
                    &mut self.feedback_out,
                    frame,
                    payload,
                    to,
                );
                self.pings += 1;
            }
        }
        busy
    }

    fn video(&mut self) -> bool {
        let mut busy = false;
        while let Some(datagram) = self.video_rx.try_recv() {
            busy = true;
            let from = datagram.peer();
            if self.peer != Some(from) {
                continue;
            }
            let Some((header, payload)) = decode(datagram.data()) else {
                continue;
            };
            if let Push::Complete(message) = self.video_in.push(&header, payload) {
                let mut ack = [0u8; ACK_LEN];
                Probe::Ack {
                    frame: message.frame.get(),
                }
                .encode(&mut ack);
                self.out_frame = self.out_frame.next();
                let frame = self.out_frame;
                send_message(
                    &mut self.feedback_tx,
                    &mut self.feedback_out,
                    frame,
                    &ack,
                    from,
                );
                self.acks += 1;
            }
        }
        busy
    }

    fn report(&self, from: SocketAddrV4) {
        say(format_args!("client {from} said bye"));
        say(format_args!("  pings echoed: {}", self.pings));
        say(format_args!("  video frames acked: {}", self.acks));
        say(format_args!(
            "  video reassembler: {:?}",
            self.video_in.stats()
        ));
        say(format_args!("  transport: {:?}", self.transport.stats()));
    }
}
