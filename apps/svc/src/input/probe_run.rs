//! Runs `input probe` (spec 0016, same method as spec 0006): feeds `count` events 2 ms apart and
//! times their arrival. Keyboard / mouse are read with Raw Input; pads with XInput (the inbox
//! `xinputhid` takes the pad away from Raw Input). Run elevated, service stopped, real mouse untouched.

use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use mixlink_core::clock;
use mixlink_input::hid::keyboard::KeyboardState;
use mixlink_input::platform::windows::vhid::VHid;

use super::open_feeder;
use super::probe::{ProbeDevice, assign_arrivals, decode_xinput_lx, period, report_for, summary};
use crate::error::SvcError;
use crate::platform::windows::{raw_input, xinput};
use crate::strings;

/// Events 2 ms apart, as in spec 0006.
const EVENT_SPACING_US: u64 = 2_000;
/// Before the first and after the last event, so nothing is cut off.
const SETTLE: Duration = Duration::from_millis(200);
const RECEIVER_START: Duration = Duration::from_secs(5);

pub fn probe(device: ProbeDevice, count: u32, slot: u8) -> Result<(), SvcError> {
    let vhid = open_feeder()?;
    let n = usize::try_from(count).unwrap_or(0);
    let mut sent = vec![0i64; n];
    let arrivals = match device {
        ProbeDevice::Pad => probe_xinput(&vhid, slot, &mut sent)?,
        ProbeDevice::Kbd | ProbeDevice::Rel | ProbeDevice::Abs => {
            probe_raw_input(&vhid, device, &mut sent)?
        }
    };
    let recv = assign_arrivals(&sent, period(device), &arrivals);
    let freq = i64::try_from(clock::ticks_per_sec()).unwrap_or(i64::MAX);
    let s = summary(&sent, &recv, freq);
    let line = strings::probe_summary(
        device_name(device),
        s.p50_us,
        s.p99_us,
        s.max_us,
        s.sent,
        s.received,
        s.lost,
    );
    writeln!(std::io::stdout(), "{line}")?;
    Ok(())
}

fn probe_raw_input(
    vhid: &VHid,
    device: ProbeDevice,
    sent: &mut [i64],
) -> Result<Vec<(u32, i64)>, SvcError> {
    let capacity = sent.len() * 2 + 16;
    let (ready_tx, ready_rx) = mpsc::channel();
    let receiver = thread::Builder::new()
        .name("input-probe-recv".into())
        .spawn(move || raw_input::receive(device, capacity, &ready_tx))?;
    let Ok(thread_id) = ready_rx.recv_timeout(RECEIVER_START) else {
        return match receiver.join() {
            Ok(Err(error)) => Err(error),
            Ok(Ok(_)) | Err(_) => Err(receiver_error("Raw Input receiver did not start")),
        };
    };
    thread::sleep(SETTLE);
    let fed = feed_events(vhid, device, 0, sent);
    thread::sleep(SETTLE);
    raw_input::quit(thread_id);
    let arrivals = receiver
        .join()
        .map_err(|_| receiver_error("Raw Input receiver panicked"))??;
    fed?;
    Ok(arrivals)
}

fn probe_xinput(vhid: &VHid, slot: u8, sent: &mut [i64]) -> Result<Vec<(u32, i64)>, SvcError> {
    let capacity = sent.len() * 2 + 16;
    let stop = Arc::new(AtomicBool::new(false));
    let receiver = {
        let stop = Arc::clone(&stop);
        thread::Builder::new()
            .name("input-probe-xinput".into())
            .spawn(move || xinput::poll_thumb_lx(slot, capacity, &stop))?
    };
    thread::sleep(SETTLE);
    let fed = feed_events(vhid, ProbeDevice::Pad, slot, sent);
    thread::sleep(SETTLE);
    stop.store(true, Ordering::Relaxed);
    let thumbs = receiver
        .join()
        .map_err(|_| receiver_error("XInput receiver panicked"))?;
    fed?;
    Ok(thumbs
        .into_iter()
        .filter_map(|(thumb, t)| decode_xinput_lx(thumb).map(|code| (code, t)))
        .collect())
}

/// One event every `EVENT_SPACING_US`, QPC taken right before each feed.
fn feed_events(
    vhid: &VHid,
    device: ProbeDevice,
    slot: u8,
    sent: &mut [i64],
) -> Result<(), SvcError> {
    let spacing = clock::ticks_per_sec() * EVENT_SPACING_US / 1_000_000;
    let mut keyboard = KeyboardState::default();
    let mut next = clock::now().raw();
    for (i, sent_at) in (0u32..).zip(sent.iter_mut()) {
        while clock::now().raw() < next {
            thread::yield_now();
        }
        let report = report_for(device, i, &mut keyboard, slot);
        *sent_at = i64::try_from(clock::now().raw()).unwrap_or(i64::MAX);
        vhid.feed(&report)?;
        next += spacing;
    }
    Ok(())
}

fn receiver_error(text: &str) -> SvcError {
    SvcError::Io(std::io::Error::other(text.to_owned()))
}

fn device_name(device: ProbeDevice) -> &'static str {
    match device {
        ProbeDevice::Kbd => "keyboard",
        ProbeDevice::Rel => "mouse relative",
        ProbeDevice::Abs => "mouse absolute",
        ProbeDevice::Pad => "Xbox pad (XInput)",
    }
}
