//! `pair` / `connect` (spec 0018): pair with a host's PIN, then run a secure session with sealed
//! pings. Uses the launcher's client files by default, so a pairing made here is the launcher's too.

use std::error::Error;
use std::io::BufRead;
use std::net::SocketAddrV4;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::thread;
use std::time::{Duration, Instant};

use mixlink_core::ids::DeviceId;
use mixlink_net::client_store;
use mixlink_net::control_link::{ControlLink, Wire};
use mixlink_net::error::NetError;
use mixlink_net::pairing::PairError;
use mixlink_net::pairing::client::{self, PairContext};
use mixlink_net::session::PING_EVERY;
use mixlink_net::session::client::connect_secure;
use mixlink_net::{RxLane, Transport, TransportConfig, TxLane};
use mixlink_proto::channel::Channel;
use mixlink_proto::control::{ControlMessage, DisplayInfo, RejectReason};
use mixlink_proto::header::WIRE_VERSION;

#[derive(Debug, Clone)]
pub struct SecureOptions {
    pub host: SocketAddrV4,
    pub id: DeviceId,
    pub store: PathBuf,
    pub seconds: u64,
}

struct Lanes {
    _transport: Transport,
    tx: TxLane,
    rx: RxLane,
}

fn lanes() -> Result<Lanes, Box<dyn Error>> {
    let (transport, mut lanes) = Transport::start(TransportConfig::client())?;
    let tx = lanes.take_tx(Channel::Control).ok_or("no control lane")?;
    let rx = lanes.take_rx(Channel::Control).ok_or("no control lane")?;
    Ok(Lanes {
        _transport: transport,
        tx,
        rx,
    })
}

fn device_name() -> String {
    let name = std::env::var("COMPUTERNAME").unwrap_or_else(|_| "net-probe".to_owned());
    name.chars().take(32).collect()
}

/// Requests a PIN, reads it from stdin, pairs; a wrong PIN asks again while the prompt is open.
pub fn pair(opts: &SecureOptions) -> Result<(), Box<dyn Error>> {
    let me = client_store::load_or_create_identity(&opts.store)?;
    let mut lanes = lanes()?;
    let cancel = AtomicBool::new(false);
    let mut wire = Wire {
        tx: &mut lanes.tx,
        rx: &mut lanes.rx,
        cancel: &cancel,
    };
    let ctx = PairContext {
        me,
        name: device_name(),
        host: opts.id,
        host_name: opts.id.to_string(),
    };
    let mut link = ControlLink::new(opts.host);
    let expires_s = client::request(&mut link, &mut wire, &ctx)?;
    eprintln!("The host shows a PIN for {expires_s} s. Type it and press Enter:");
    for line in std::io::stdin().lock().lines() {
        let line = line?;
        let Some(pin) = parse_pin(&line) else {
            eprintln!("The PIN is 6 digits. Type it again:");
            continue;
        };
        match client::submit(&mut link, &mut wire, &ctx, pin) {
            Ok(known) => {
                client_store::remember(&opts.store, known)?;
                eprintln!("Paired. Saved in {}", opts.store.display());
                return Ok(());
            }
            Err(PairError::BadPin) => eprintln!("Wrong PIN. Type it again:"),
            Err(error) => return Err(error.into()),
        }
    }
    Err("stdin closed before a PIN was typed".into())
}

fn parse_pin(text: &str) -> Option<u32> {
    let text = text.trim();
    (text.len() == 6 && text.bytes().all(|b| b.is_ascii_digit()))
        .then(|| text.parse().ok())
        .flatten()
}

fn hello(me: DeviceId) -> ControlMessage {
    ControlMessage::Hello {
        proto: WIRE_VERSION,
        device: me,
        name: device_name(),
        displays: vec![DisplayInfo {
            width: 1920,
            height: 1080,
            refresh_mhz: 60_000,
            hdr: false,
            dpi: 96,
        }],
        stream_display: 0,
    }
}

/// A secure session for `seconds`, one sealed ping per second; prints the round trips.
pub fn connect(opts: &SecureOptions) -> Result<(), Box<dyn Error>> {
    let me = client_store::load_or_create_identity(&opts.store)?;
    let known = client_store::load_known(&opts.store)
        .into_iter()
        .find(|known| known.host == opts.id)
        .ok_or("this host is not paired; run `net-probe pair` first")?;
    let mut lanes = lanes()?;
    let cancel = AtomicBool::new(false);
    let mut wire = Wire {
        tx: &mut lanes.tx,
        rx: &mut lanes.rx,
        cancel: &cancel,
    };
    let link = ControlLink::new(opts.host);
    let mut session = match connect_secure(link, &mut wire, &me, &known, &hello(me.id)) {
        Ok(session) => session,
        Err(NetError::Rejected(RejectReason::NotPaired)) => {
            client_store::forget(&opts.store, opts.id)?;
            return Err(
                "the host no longer knows this client; it was forgotten here, pair again".into(),
            );
        }
        Err(error) => return Err(error.into()),
    };
    eprintln!(
        "session {} up, mode {}x{}",
        session.id.get(),
        session.mode.width,
        session.mode.height
    );
    let mut rtts = Vec::new();
    let end = Instant::now() + Duration::from_secs(opts.seconds);
    while Instant::now() < end {
        let rtt = session.ping(&mut wire)?;
        rtts.push(rtt);
        thread::sleep(PING_EVERY.saturating_sub(rtt));
    }
    session.bye(&mut wire)?;
    rtts.sort();
    let at = |q: usize| rtts.get((rtts.len() * q / 100).min(rtts.len().saturating_sub(1)));
    if let (Some(p50), Some(p99)) = (at(50), at(99)) {
        eprintln!(
            "{} pings, rtt p50 {} us, p99 {} us",
            rtts.len(),
            p50.as_micros(),
            p99.as_micros()
        );
    }
    Ok(())
}
