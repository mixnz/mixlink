use std::sync::Arc;
use std::thread;
use std::time::Duration;

use mixlink_core::config::Topology;

use super::*;
use crate::display::fakes::*;

fn no_sleep(_: Duration) {}

/// Runs `serve` on this thread (the fakes are not `Send`) while `client` drives it from another.
fn run_serve<T: Send + 'static>(
    r: &mut Rig,
    client: impl FnOnce(DisplayHandle) -> T + Send + 'static,
) -> T {
    let (handle, inbox) = DisplayHandle::channel();
    let stop = Arc::new(StopSignal::new());
    let stopper = Arc::clone(&stop);
    let worker = thread::spawn(move || {
        let out = client(handle);
        stopper.stop();
        out
    });
    let mut headless = None;
    serve(
        &mut ports(r),
        &mut headless,
        Some(Topology::VirtualOnly),
        &inbox,
        &stop,
        &no_sleep,
    );
    worker.join().unwrap()
}

#[test]
fn begin_then_end_restores() {
    let mut r = rig("requests-begin-end");
    let ended = run_serve(&mut r, |display| {
        let target = display.begin(client(), MODE).unwrap();
        assert_eq!(target.active.client, client());
        assert_eq!(target.connector, target.active.monitor - 1);
        display.end(target.active)
    });
    assert!(ended.is_ok());
    let log = r.log.borrow();
    assert!(log.iter().any(|l| l.starts_with("add ")), "{log:?}");
    assert!(log.iter().any(|l| l.starts_with("switch ")), "{log:?}");
    assert!(log.iter().any(|l| l.starts_with("remove ")), "{log:?}");
    assert!(log.iter().any(|l| l.starts_with("restore ")), "{log:?}");
    assert!(r.driver.monitors.is_empty());
}

#[test]
fn begin_error_is_returned() {
    let mut r = rig("requests-begin-error");
    r.config.fail_switch = true;
    let result = run_serve(&mut r, |display| display.begin(client(), MODE).map(|_| ()));
    assert!(
        matches!(result, Err(SvcError::Topology { .. })),
        "{result:?}"
    );
    assert!(r.driver.monitors.is_empty(), "begin undoes the monitor");
}

#[test]
fn stop_ends_serve() {
    let mut r = rig("requests-stop");
    run_serve(&mut r, |_| ());
    assert!(r.log.borrow().is_empty());
}

#[test]
fn begin_after_thread_gone_is_not_ready() {
    let (handle, inbox) = DisplayHandle::channel();
    drop(inbox);
    assert!(matches!(
        handle.begin(client(), MODE),
        Err(SvcError::DisplayNotReady)
    ));
    assert!(matches!(
        handle.end(Active {
            monitor: 1,
            client: client()
        }),
        Err(SvcError::DisplayNotReady)
    ));
}
