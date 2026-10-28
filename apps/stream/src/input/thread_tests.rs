use std::time::Duration;

use mixlink_net::{Transport, TransportConfig};
use mixlink_proto::channel::Channel;
use mixlink_proto::crypto::{KEY_LEN, SessionKeys};

use super::*;
use crate::net::drop_knob::DropKnob;

/// A key from the hook while the thread waits with no deadline must wake it, so the repeats and the
/// keep-alive of a held key get scheduled (the host releases a key after 500 ms of silence).
#[test]
fn hooked_key_wakes_the_wait() {
    let (host, _host_lanes) = Transport::start(TransportConfig::loopback()).unwrap();
    let (_client, mut lanes) = Transport::start(TransportConfig::loopback()).unwrap();
    let keys = SessionKeys {
        c2h: [3; KEY_LEN],
        h2c: [4; KEY_LEN],
    };
    let tx = InputTx::new(
        lanes.take_tx(Channel::Input).unwrap(),
        &keys,
        host.local_addr(),
        DropKnob::default(),
    );
    let (_ring_tx, ring) = RingBuffer::new(4);
    let pump = Pump::new().unwrap();
    install_worker(tx, ring, Box::new(|_| {}), pump.waker());
    with_worker(|w| w.captured = true);
    assert_eq!(with_worker(|w| w.tick(Instant::now())), Some(None));
    // As the hook would call it, from inside the wait.
    let swallowed = on_key(RawKey {
        scan: 0x1E,
        extended: false,
        vk: 0x41,
        down: true,
    });
    assert!(swallowed);
    let deadline = Instant::now() + Duration::from_millis(200);
    assert_eq!(pump.wait(Some(deadline)), Pumped::Woken);
    let next = with_worker(|w| w.tick(Instant::now())).flatten();
    assert!(next.is_some(), "a repeat is scheduled");
    WORKER.with(|w| w.borrow_mut().take());
}
