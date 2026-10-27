//! `net-probe run`: handshake, then synthetic video + 1000 pings / s; reports RTT and frame loss.

use std::error::Error;
use std::net::{Ipv4Addr, SocketAddrV4};
use std::sync::atomic::AtomicBool;
use std::thread;
use std::time::{Duration, Instant};

use mixlink_core::clock::{self, Ticks};
use mixlink_core::ids::{DeviceId, FrameId};
use mixlink_net::error::NetError;
use mixlink_net::handshake::{self, HandshakeReply};
use mixlink_net::{RxLane, Transport, TransportConfig, TxLane};
use mixlink_proto::channel::Channel;
use mixlink_proto::control::{self, ByeReason, ControlMessage, DisplayInfo};
use mixlink_proto::header::WIRE_VERSION;
use mixlink_proto::packetizer::Packetizer;

use crate::link::{decode, say, send_message};
use crate::wire::{PING_LEN, Probe};

#[derive(Debug, Clone, Copy)]
pub struct RunOptions {
    pub host: Ipv4Addr,
    pub port: u16,
    pub seconds: u64,
    pub mbps: u64,
    pub fps: u64,
}

#[derive(Debug, Default)]
struct SendReport {
    frames: u64,
    abandoned: u64,
    pings: u64,
}

impl SendReport {
    /// One id per attempt, abandoned ones included: a reused id could let the host complete a
    /// frame from fragments of two attempts. Wraps like `FrameId`.
    fn next_frame_id(&self) -> FrameId {
        FrameId::new((self.frames + self.abandoned) as u32)
    }
}

#[derive(Debug, Default)]
struct Feedback {
    rtt: Vec<Duration>,
    acks: u64,
}

pub fn run(opts: RunOptions) -> Result<(), Box<dyn Error>> {
    let (transport, mut lanes) = Transport::start(TransportConfig::client())?;
    let lane_missing = "lane already taken";
    let mut control_tx = lanes.take_tx(Channel::Control).ok_or(lane_missing)?;
    let mut control_rx = lanes.take_rx(Channel::Control).ok_or(lane_missing)?;
    let video_tx = lanes.take_tx(Channel::Video).ok_or(lane_missing)?;
    let input_tx = lanes.take_tx(Channel::Input).ok_or(lane_missing)?;
    let mut feedback_rx = lanes.take_rx(Channel::Feedback).ok_or(lane_missing)?;
    let host = SocketAddrV4::new(opts.host, opts.port);

    let mode = DisplayInfo {
        width: 2560,
        height: 1440,
        refresh_mhz: 120_000,
        hdr: false,
        dpi: 96,
    };
    let hello = ControlMessage::Hello {
        proto: WIRE_VERSION,
        device: DeviceId::from_bytes([7; 16]),
        name: "net-probe".into(),
        displays: vec![mode],
        stream_display: 0,
        codecs: vec![mixlink_proto::control::CodecId::H264],
    };
    let started = Instant::now();
    let cancel = AtomicBool::new(false);
    match handshake::connect(&mut control_tx, &mut control_rx, host, &hello, &cancel)? {
        HandshakeReply::Welcome { session, mode } => say(format_args!(
            "handshake: {} ms, session {}, mode {}x{} @ {} mHz",
            started.elapsed().as_millis(),
            session.get(),
            mode.width,
            mode.height,
            mode.refresh_mhz
        )),
        HandshakeReply::Reject(reason) => {
            say(format_args!("host rejected the client: {reason:?}"));
            return Ok(());
        }
    }

    let sender = thread::Builder::new()
        .name("net-probe-send".into())
        .spawn(move || send_load(video_tx, input_tx, host, opts))
        .map_err(NetError::Spawn)?;
    let mut feedback = Feedback::default();
    while !sender.is_finished() {
        read_feedback(
            &mut feedback_rx,
            host,
            &mut feedback,
            Duration::from_millis(10),
        );
    }
    let sent = sender.join().unwrap_or_default();
    let bye = control::encode(&ControlMessage::Bye {
        reason: ByeReason::UserClosed,
    })?;
    let mut control_out = Packetizer::new(Channel::Control);
    send_message(
        &mut control_tx,
        &mut control_out,
        FrameId::new(1_000),
        &bye,
        host,
    );
    let grace = Instant::now() + Duration::from_millis(500);
    while Instant::now() < grace {
        read_feedback(
            &mut feedback_rx,
            host,
            &mut feedback,
            Duration::from_millis(10),
        );
    }
    report(&sent, &mut feedback, &transport);
    Ok(())
}

/// Sender thread: one synthetic frame per `1 / fps`, one ping per ms, until `seconds` end.
fn send_load(
    mut video_tx: TxLane,
    mut input_tx: TxLane,
    host: SocketAddrV4,
    opts: RunOptions,
) -> SendReport {
    let frame_len = usize::try_from(opts.mbps * 1_000_000 / 8 / opts.fps).unwrap_or(0);
    let frame: Vec<u8> = (0..frame_len).map(|i| (i % 251) as u8).collect();
    let frame_every = Duration::from_secs(1) / u32::try_from(opts.fps).unwrap_or(1);
    let ping_every = Duration::from_millis(1);
    let mut video_out = Packetizer::new(Channel::Video);
    let mut input_out = Packetizer::new(Channel::Input);
    let mut report = SendReport::default();
    let start = Instant::now();
    let end = start + Duration::from_secs(opts.seconds);
    let (mut next_frame, mut next_ping) = (start, start);
    let mut ping = [0u8; PING_LEN];
    loop {
        let now = Instant::now();
        if now >= end {
            break;
        }
        if now >= next_frame {
            let id = report.next_frame_id();
            if send_message(&mut video_tx, &mut video_out, id, &frame, host) {
                report.frames += 1;
            } else {
                report.abandoned += 1;
            }
            next_frame += frame_every;
        }
        if now >= next_ping {
            let seq = u32::try_from(report.pings).unwrap_or(u32::MAX);
            Probe::Ping {
                seq,
                sent: clock::now().raw(),
            }
            .encode(&mut ping);
            send_message(
                &mut input_tx,
                &mut input_out,
                FrameId::new(seq),
                &ping,
                host,
            );
            report.pings += 1;
            next_ping += ping_every;
        }
        let wake = next_frame.min(next_ping);
        if let Some(wait) = wake.checked_duration_since(Instant::now()) {
            thread::sleep(wait);
        }
    }
    report
}

fn read_feedback(rx: &mut RxLane, host: SocketAddrV4, feedback: &mut Feedback, wait: Duration) {
    let Some(datagram) = rx.recv_timeout(wait) else {
        return;
    };
    if datagram.peer() != host {
        return;
    }
    let Some((_, payload)) = decode(datagram.data()) else {
        return;
    };
    match Probe::decode(payload) {
        Some(Probe::Ping { sent, .. }) => {
            let rtt = datagram.at().duration_since(Ticks::from_raw(sent));
            feedback.rtt.push(rtt);
        }
        Some(Probe::Ack { .. }) => feedback.acks += 1,
        None => {}
    }
}

fn report(sent: &SendReport, feedback: &mut Feedback, transport: &Transport) {
    let lost = sent.frames.saturating_sub(feedback.acks);
    let lost_pct = if sent.frames == 0 {
        0.0
    } else {
        lost as f64 * 100.0 / sent.frames as f64
    };
    say(format_args!(
        "video: {} frames sent, {} acked, {} lost ({lost_pct:.2} %), {} abandoned (lane full)",
        sent.frames, feedback.acks, lost, sent.abandoned
    ));
    feedback.rtt.sort();
    let pick = |q: usize| {
        let n = feedback.rtt.len();
        feedback
            .rtt
            .get((n * q / 100).min(n.saturating_sub(1)))
            .map_or(0, |d| d.as_micros())
    };
    say(format_args!(
        "pings: {} sent, {} answered; RTT p50 {} us, p99 {} us, max {} us",
        sent.pings,
        feedback.rtt.len(),
        pick(50),
        pick(99),
        pick(100)
    ));
    say(format_args!("transport: {:?}", transport.stats()));
}

#[cfg(test)]
#[path = "run_tests.rs"]
mod tests;
