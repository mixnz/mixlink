//! s3-latency: feed call -> input visible to apps (spec 0006 cases D, E).
//!   s3-latency <vhid|sendinput|vigem> <kbd|rel|abs|pad|wgi> [--count N]

use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use s3_vhid::feed::{Backend, FeedError, Feeder};
use s3_vhid::input::Input;
use s3_vhid::log;
use s3_vhid::platform::{
    self, devices, hid::VhidFeeder, poll, qpc, rawinput, timer::Pacer, vigem::VigemFeeder,
};
use s3_vhid::report::PadState;
use s3_vhid::seq::{DeviceFilter, Probe, input_for};
use s3_vhid::stats::{Samples, summary};

const USAGE: &str = "usage: s3-latency <vhid|sendinput|vigem> <kbd|rel|abs|pad|wgi> [--count N]";
const SPACING_US: i64 = 2_000;

enum Stop {
    Thread(u32),
    Flag(Arc<AtomicBool>),
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut lines = vec![format!("s3-latency {}", args.join(" "))];
    let result = run(&args, &mut lines);
    if let Err(e) = &result {
        lines.push(format!("ERROR {e}"));
    }
    for line in &lines {
        println!("{line}");
    }
    match log::write_text(
        &log::run_name(&format!("latency-{}", args.join("-"))),
        &lines,
    ) {
        Ok(path) => println!("Log: {}", path.display()),
        Err(e) => eprintln!("log not written: {e}"),
    }
    if result.is_ok() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn run(args: &[String], lines: &mut Vec<String>) -> Result<(), String> {
    let backend = args.first().and_then(|a| Backend::parse(a)).ok_or(USAGE)?;
    let probe = args.get(1).and_then(|p| Probe::parse(p)).ok_or(USAGE)?;
    let n = match args.iter().position(|a| a == "--count") {
        Some(p) => args.get(p + 1).and_then(|c| c.parse().ok()).ok_or(USAGE)?,
        None => 10_000usize,
    };
    let ok = matches!(
        (backend, probe),
        (Backend::Vhid, _)
            | (Backend::SendInput, Probe::Kbd | Probe::Rel | Probe::Abs)
            | (Backend::Vigem, Probe::Pad)
    );
    if !ok {
        return Err(format!("{backend:?} cannot feed {probe:?}"));
    }
    let err = |e: FeedError| e.to_string();

    let mut setup = open(backend, probe)?;
    let (stop, receiver) = start_receiver(probe, setup.filter, n, setup.user_index)?;
    thread::sleep(Duration::from_millis(300));

    let mut sendinput = platform::sendinput::SendInputFeeder;
    let feeder: &mut dyn Feeder = match (setup.vhid.as_mut(), setup.vigem.as_mut()) {
        (Some(f), _) => f,
        (None, Some(f)) => f,
        (None, None) => &mut sendinput,
    };
    let (sent, send_error) = send_all(feeder, probe, n)?;
    thread::sleep(Duration::from_millis(300));
    let vhid = setup.vhid;

    match stop {
        Stop::Thread(id) => platform::rawinput::quit(id),
        Stop::Flag(flag) => flag.store(true, Ordering::Relaxed),
    }
    let recv = receiver
        .join()
        .map_err(|_| "receiver panicked".to_string())?
        .map_err(err)?;
    let sum = summary(&Samples::merge(sent, recv), qpc::freq());
    lines.push(format!(
        "backend {backend:?}, probe {probe:?}, events {n}, spacing {SPACING_US} us"
    ));
    lines.push(log::summary_line(&sum));
    if let Some(v) = &vhid {
        lines.push(format!(
            "driver dropped (ring full): {}",
            v.read_dropped().map_err(err)?
        ));
    }
    let pass = sum.p99_us <= 1000.0 && (backend != Backend::Vhid || sum.lost == 0);
    lines.push(format!("RESULT {}", if pass { "PASS" } else { "FAIL" }));
    match send_error {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

struct Setup {
    vhid: Option<VhidFeeder>,
    vigem: Option<VigemFeeder>,
    filter: DeviceFilter,
    user_index: Option<u32>,
}

/// Opens the feeder and decides which Raw Input device the receiver accepts.
fn open(backend: Backend, probe: Probe) -> Result<Setup, String> {
    let err = |e: FeedError| e.to_string();
    let mut setup = Setup {
        vhid: None,
        vigem: None,
        filter: DeviceFilter::new(),
        user_index: None,
    };
    match backend {
        Backend::Vhid => {
            let col = match probe {
                Probe::Kbd => 1,
                Probe::Rel => 2,
                Probe::Abs => 3,
                Probe::Pad | Probe::Wgi => 4,
            };
            let list = devices::list().map_err(err)?;
            let handle = devices::vhid_handle(&list, col)
                .ok_or(format!("vhid collection {col} not found"))?;
            setup.filter.add(handle);
            setup.vhid = Some(VhidFeeder::open().map_err(err)?);
        }
        Backend::SendInput => {
            setup.filter.add(0);
        }
        Backend::Vigem => {
            let mut v = VigemFeeder::connect().map_err(err)?;
            setup.user_index = Some(v.user_index().map_err(err)?);
            setup.vigem = Some(v);
        }
    }
    Ok(setup)
}

/// Sends `n` events 2 ms apart on this thread; returns send times and the first send error.
fn send_all(
    feeder: &mut dyn Feeder,
    probe: Probe,
    n: usize,
) -> Result<(Samples, Option<String>), String> {
    platform::set_time_critical();
    let pacer = Pacer::new().map_err(|e| e.to_string())?;
    let mut sent = Samples::with_capacity(n);
    let mut send_error = None;
    for i in 0..n {
        pacer.sleep_us(SPACING_US).map_err(|e| e.to_string())?;
        // HOT PATH
        let input = input_for(probe, i as u32);
        sent.record_sent(i, qpc::now());
        if let Err(e) = feeder.send(&input) {
            send_error = Some(format!("event {i}: {e}"));
            break;
        }
    }
    if matches!(probe, Probe::Pad | Probe::Wgi) {
        let _ = feeder.send(&Input::Pad(PadState::CENTERED));
    }
    Ok((sent, send_error))
}

type Receiver = JoinHandle<Result<Samples, FeedError>>;

fn start_receiver(
    probe: Probe,
    filter: DeviceFilter,
    n: usize,
    user_index: Option<u32>,
) -> Result<(Stop, Receiver), String> {
    match (probe, user_index) {
        (Probe::Pad, Some(index)) => {
            let flag = Arc::new(AtomicBool::new(false));
            let f = flag.clone();
            let h = thread::spawn(move || {
                platform::set_time_critical();
                Ok(poll::xinput(index, n, f))
            });
            Ok((Stop::Flag(flag), h))
        }
        (Probe::Wgi, _) => {
            let flag = Arc::new(AtomicBool::new(false));
            let f = flag.clone();
            let (tx, rx) = mpsc::channel();
            let h = thread::spawn(move || {
                let controller = poll::find_wgi(0xDEED, 0xFEED);
                let _ = tx.send(controller.is_ok());
                platform::set_time_critical();
                poll::wgi(controller?, n, f)
            });
            match rx.recv() {
                Ok(true) => Ok((Stop::Flag(flag), h)),
                _ => Err(h
                    .join()
                    .ok()
                    .and_then(|r| r.err())
                    .map_or("WGI receiver failed".into(), |e| e.to_string())),
            }
        }
        _ => {
            let (tx, rx) = mpsc::channel();
            let h = thread::spawn(move || {
                platform::set_time_critical();
                rawinput::receive(probe, filter, n, tx)
            });
            match rx.recv() {
                Ok(id) => Ok((Stop::Thread(id), h)),
                Err(_) => Err(h
                    .join()
                    .ok()
                    .and_then(|r| r.err())
                    .map_or("Raw Input receiver failed".into(), |e| e.to_string())),
            }
        }
    }
}
