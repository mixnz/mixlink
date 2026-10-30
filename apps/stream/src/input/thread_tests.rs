use std::time::Duration;

use mixlink_net::{Transport, TransportConfig};
use mixlink_proto::channel::Channel;
use mixlink_proto::crypto::{KEY_LEN, SessionKeys};

use super::*;
use crate::input::mode::KeyActivity;
use crate::net::drop_knob::DropKnob;
fn worker() -> (Transport, Worker) {
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
    (host, Worker::new(tx, ring, Box::new(|_| {}), pump.waker()))
}

fn raw(scan: u16, vk: u16, down: bool) -> RawKey {
    RawKey {
        scan,
        extended: false,
        vk,
        down,
    }
}

#[test]
fn hooked_key_changes_state_once() {
    let (_host, mut w) = worker();
    assert!(w.key(raw(0x1E, 0x41, true)));
    assert!(!w.key(raw(0x1E, 0x41, true)), "a repeat is no change");
    assert!(w.state.held());
}

#[test]
fn release_shortcut_releases_held_keys() {
    let (_host, mut w) = worker();
    for (scan, vk) in [(0x1D, 0xA2), (0x38, 0xA4), (0x2A, 0xA0)] {
        w.key(raw(scan, vk, true));
    }
    assert!(
        w.key(raw(0x2C, 0x5A, true)),
        "Ctrl+Alt+Shift+Z releases the held modifiers"
    );
    assert!(!w.state.held());
}

/// By hand: `cargo test -p mixlink-stream thread_hook_sees -- --ignored --nocapture`. The real thread:
/// capture on, two injected F24 keys, capture off; the log line `keyboard hook off` shows the counts.
#[test]
#[ignore = "needs an interactive desktop"]
fn thread_hook_sees_injected() {
    use std::mem::size_of;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_KEYUP, SendInput,
        VK_F24,
    };
    let _ = env_logger::builder()
        .is_test(true)
        .filter_level(log::LevelFilter::Info)
        .try_init();
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
    let (thread, mut feed) = InputThread::start(tx, KeyActivity::new(), |_| {}).unwrap();
    assert!(feed.push(MouseEvent::Capture(true)));
    std::thread::sleep(Duration::from_millis(100));
    let key = |flags| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VK_F24,
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    let inputs = [key(KEYBD_EVENT_FLAGS(0)), key(KEYEVENTF_KEYUP)];
    // SAFETY: two valid keyboard INPUTs of the declared size.
    let sent = unsafe { SendInput(&inputs, size_of::<INPUT>() as i32) };
    std::thread::sleep(Duration::from_millis(200));
    assert!(feed.push(MouseEvent::Capture(false)));
    std::thread::sleep(Duration::from_millis(100));
    log::info!("sent {sent}");
    thread.stop();
}

/// By hand: `cargo test -p mixlink-stream thread_hook_physical -- --ignored --nocapture`, then type on
/// this PC for 30 s. The real thread with input captured; the log line `keyboard hook off` shows what
/// its hook saw.
#[test]
#[ignore = "needs a person typing"]
fn thread_hook_physical() {
    let _ = env_logger::builder()
        .is_test(true)
        .filter_level(log::LevelFilter::Info)
        .try_init();
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
    let (thread, mut feed) = InputThread::start(tx, KeyActivity::new(), |_| {}).unwrap();
    assert!(feed.push(MouseEvent::Capture(true)));
    log::info!("captured: type for 30 s");
    std::thread::sleep(Duration::from_secs(30));
    assert!(feed.push(MouseEvent::Capture(false)));
    std::thread::sleep(Duration::from_millis(200));
    thread.stop();
}

#[test]
fn window_key_counts_only_while_captured() {
    let (_host, mut w) = worker();
    assert!(!w.mouse(MouseEvent::Key(raw(0x1E, 0, true))));
    w.captured = true;
    assert!(w.mouse(MouseEvent::Key(raw(0x1E, 0, true))));
    assert!(
        w.activity.last().is_some(),
        "typing is noted for the mouse mode"
    );
}
