use std::cell::RefCell;
use std::net::Ipv4Addr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use mixlink_net::{Transport, TransportConfig};
use mixlink_proto::channel::Channel;

use super::*;
use crate::stream::video_send::tests::keys;

fn lanes() -> (Transport, StreamLanes) {
    let (transport, mut lanes) = Transport::start(TransportConfig::loopback()).unwrap();
    let lanes = StreamLanes {
        video: lanes.take_tx(Channel::Video).unwrap(),
        cursor: lanes.take_tx(Channel::Cursor).unwrap(),
        feedback: lanes.take_rx(Channel::Feedback).unwrap(),
    };
    (transport, lanes)
}

fn start(byte: u8, port: u16) -> StreamStart {
    StreamStart {
        device: DeviceId::from_bytes([byte; 16]),
        peer: SocketAddrV4::new(Ipv4Addr::LOCALHOST, port),
        keys: keys(),
        mode: DisplayInfo {
            width: 1920,
            height: 1080,
            refresh_mhz: 60_000,
            hdr: false,
            dpi: 96,
        },
        codec: CodecId::H264,
        video: VideoConfig::default(),
    }
}

/// Waits for `f` to give `Some`, up to 2 s.
fn soon<T>(mut f: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if let Some(value) = f() {
            return value;
        }
        assert!(Instant::now() < deadline, "timed out");
        thread::sleep(Duration::from_millis(5));
    }
}

fn wait_for_stop(lanes: StreamLanes, stop: &AtomicBool) -> (StreamLanes, StreamEnd) {
    while !stop.load(Ordering::Acquire) {
        thread::sleep(Duration::from_millis(2));
    }
    (lanes, StreamEnd::Stopped)
}

#[test]
fn lanes_return_after_stop() {
    let (_transport, lanes) = lanes();
    let mut streamer = Streamer::spawn(lanes, wait_for_stop).unwrap();
    thread::sleep(Duration::from_millis(20));
    assert!(streamer.poll_done().is_none(), "still running");
    streamer.stop();
    let (_, end) = soon(|| streamer.poll_done());
    assert!(matches!(end, StreamEnd::Stopped));
}

struct FakeMonitor {
    fail: bool,
    log: RefCell<Vec<String>>,
}

impl MonitorPort for FakeMonitor {
    fn begin(&self, _client: DeviceId, mode: DisplayInfo) -> Result<StreamTarget, SvcError> {
        self.log.borrow_mut().push(format!("begin {}", mode.width));
        if self.fail {
            return Err(SvcError::DisplayNotReady);
        }
        Ok(StreamTarget {
            active: Active {
                monitor: 2,
                client: DeviceId::from_bytes([1; 16]),
            },
            connector: 1,
        })
    }

    fn end(&self, active: Active) -> Result<(), SvcError> {
        self.log
            .borrow_mut()
            .push(format!("end {}", active.monitor));
        Ok(())
    }
}

#[test]
fn begin_failure_is_failed_end() {
    let (_transport, lanes) = lanes();
    let monitor = FakeMonitor {
        fail: true,
        log: RefCell::default(),
    };
    let (_, end) = run_session(&monitor, &start(1, 9), lanes, |_, _| {
        panic!("no capture without a monitor")
    });
    assert!(matches!(end, StreamEnd::Failed(SvcError::DisplayNotReady)));
    assert_eq!(*monitor.log.borrow(), ["begin 1920"]);
}

#[test]
fn capture_error_still_ends_monitor() {
    let (_transport, lanes) = lanes();
    let monitor = FakeMonitor {
        fail: false,
        log: RefCell::default(),
    };
    let (_, end) = run_session(&monitor, &start(1, 9), lanes, |target, lanes| {
        assert_eq!(target.connector, 1);
        (lanes, Err(SvcError::DisplayNotReady))
    });
    assert!(matches!(end, StreamEnd::Failed(_)));
    assert_eq!(*monitor.log.borrow(), ["begin 1920", "end 2"]);
}

type Log = Arc<Mutex<Vec<String>>>;

fn control(
    log: &Log,
    lanes: StreamLanes,
) -> StreamControl<impl FnMut(StreamStart, StreamLanes) -> io::Result<Streamer>> {
    let log = Arc::clone(log);
    StreamControl::new(lanes, move |start: StreamStart, lanes| {
        let port = start.peer.port();
        log.lock().unwrap().push(format!("start {port}"));
        let log = Arc::clone(&log);
        Streamer::spawn(lanes, move |lanes, stop| {
            let out = wait_for_stop(lanes, stop);
            log.lock().unwrap().push(format!("end {port}"));
            out
        })
    })
}

#[test]
fn restart_waits_for_lanes() {
    let (_transport, lanes) = lanes();
    let log: Log = Arc::default();
    let mut control = control(&log, lanes);
    control.started(start(1, 1));
    // Same device on a new address: the old stream must give the lanes back first.
    control.started(start(1, 2));
    let (device, end) = soon(|| control.poll());
    assert_eq!(device, DeviceId::from_bytes([1; 16]));
    assert!(matches!(end, StreamEnd::Stopped));
    assert_eq!(*log.lock().unwrap(), ["start 1", "end 1", "start 2"]);
    control.shutdown();
    assert_eq!(
        log.lock().unwrap().last().map(String::as_str),
        Some("end 2")
    );
}

#[test]
fn ended_stops_running_and_drops_pending() {
    let (_transport, lanes) = lanes();
    let log: Log = Arc::default();
    let mut control = control(&log, lanes);
    control.started(start(1, 1));
    control.started(start(2, 2));
    control.ended(DeviceId::from_bytes([2; 16]));
    soon(|| control.poll());
    thread::sleep(Duration::from_millis(20));
    assert!(control.poll().is_none());
    assert_eq!(*log.lock().unwrap(), ["start 1", "end 1"]);
    control.ended(DeviceId::from_bytes([1; 16]));
    control.started(start(3, 3));
    control.ended(DeviceId::from_bytes([3; 16]));
    soon(|| control.poll());
    assert_eq!(
        log.lock().unwrap().len(),
        4,
        "the lanes came back, a new session streams"
    );
}
