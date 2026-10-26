//! Broadcast discovery over loopback (spec 0017): an `Advertiser` answering a `Browser` that
//! probes fixed targets, plus responder robustness.
// Helpers outside `#[test]` fns are not covered by `allow-unwrap-in-tests`.
#![allow(clippy::unwrap_used)]

use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use mixlink_core::ids::DeviceId;
use mixlink_net::discovery::wire::{PROBE, decode_reply};
use mixlink_net::discovery::{
    Advert, Advertiser, AdvertiserConfig, Browser, BrowserConfig, CAPS_V1, EXPIRY, HostEntry,
    ProbeTargets,
};

const LOCALHOST: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);

fn advert(id: u8) -> Advert {
    Advert {
        id: DeviceId::from_bytes([id; 16]),
        name: format!("Loopback {id}"),
        ver: 1,
        caps: CAPS_V1.to_owned(),
        port: 47801,
    }
}

fn advertiser(id: u8) -> Advertiser {
    let cfg = AdvertiserConfig {
        discovery_port: 0,
        mdns: false,
    };
    Advertiser::start(advert(id), cfg).unwrap()
}

fn browser(targets: Vec<SocketAddr>, on_change: Box<dyn FnMut(Vec<HostEntry>) + Send>) -> Browser {
    let cfg = BrowserConfig {
        discovery_port: 0,
        probe_every: Duration::from_millis(100),
        targets: ProbeTargets::Fixed(targets),
        mdns: false,
    };
    Browser::start(cfg, on_change).unwrap()
}

fn wait_until(limit: Duration, mut done: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + limit;
    while Instant::now() < deadline {
        if done() {
            return true;
        }
        thread::sleep(Duration::from_millis(20));
    }
    done()
}

fn target(adv: &Advertiser) -> SocketAddr {
    SocketAddr::new(LOCALHOST, adv.local_port().unwrap())
}

/// Sends a probe from a fresh socket; the decoded reply, if one arrives within `wait`.
fn probe(to: SocketAddr, wait: Duration) -> Option<Advert> {
    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    socket.set_read_timeout(Some(wait)).unwrap();
    socket.send_to(&PROBE, to).unwrap();
    let mut buf = [0u8; 1500];
    let (len, _) = socket.recv_from(&mut buf).ok()?;
    decode_reply(&buf[..len])
}

#[test]
fn browser_finds_advertiser() {
    let adv = advertiser(1);
    let b = browser(vec![target(&adv)], Box::new(|_| {}));
    assert!(wait_until(Duration::from_secs(2), || b.snapshot().len() == 1));
    let entry = &b.snapshot()[0];
    assert_eq!(entry.id, advert(1).id);
    assert_eq!(entry.address, LOCALHOST);
    b.stop();
    adv.stop();
}

#[test]
fn browser_drops_stopped_advertiser() {
    let adv = advertiser(2);
    let changes = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&changes);
    let b = browser(
        vec![target(&adv)],
        Box::new(move |_| {
            counter.fetch_add(1, Ordering::SeqCst);
        }),
    );
    assert!(wait_until(Duration::from_secs(2), || b.snapshot().len() == 1));
    adv.stop();
    assert!(wait_until(EXPIRY + Duration::from_secs(1), || b
        .snapshot()
        .is_empty()));
    assert!(changes.load(Ordering::SeqCst) >= 2);
    b.stop();
}

#[test]
fn responder_survives_closed_peer() {
    let adv = advertiser(3);
    let to = target(&adv);
    {
        let gone = UdpSocket::bind("127.0.0.1:0").unwrap();
        gone.send_to(&PROBE, to).unwrap();
    }
    // Past the per-source reply window, so the next probe is not rate limited.
    thread::sleep(Duration::from_millis(400));
    assert_eq!(probe(to, Duration::from_secs(1)), Some(advert(3)));
    adv.stop();
}

#[test]
fn responder_ignores_garbage() {
    let adv = advertiser(4);
    let to = target(&adv);
    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    socket
        .set_read_timeout(Some(Duration::from_millis(300)))
        .unwrap();
    for i in 0..100u8 {
        let junk: Vec<u8> = (0..(i % 40))
            .map(|j| j.wrapping_mul(i).wrapping_add(7))
            .collect();
        socket.send_to(&junk, to).unwrap();
    }
    let mut buf = [0u8; 1500];
    assert!(socket.recv_from(&mut buf).is_err(), "garbage got a reply");
    assert_eq!(probe(to, Duration::from_secs(1)), Some(advert(4)));
    adv.stop();
}

#[test]
fn browser_with_no_targets_stays_empty() {
    let changes = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&changes);
    let b = browser(
        Vec::new(),
        Box::new(move |_| {
            counter.fetch_add(1, Ordering::SeqCst);
        }),
    );
    thread::sleep(Duration::from_millis(500));
    assert!(b.snapshot().is_empty());
    assert_eq!(changes.load(Ordering::SeqCst), 0);
    b.stop();
}
