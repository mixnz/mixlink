use std::collections::VecDeque;
use std::time::{Duration, Instant};

use mixlink_capture::{CaptureError, CaptureEvent, Frame, FrameSource};
use mixlink_core::clock::Ticks;

use super::*;
use crate::capture::report::Stats;

#[derive(Debug, Clone, Copy)]
enum Step {
    Idle,
    Reopen(u32, u32),
    Frame(u64),
    Stop,
}

#[derive(Debug)]
struct FakeFrame(u64);

impl Frame for FakeFrame {
    fn seq(&self) -> u64 {
        self.0
    }
    fn width(&self) -> u32 {
        64
    }
    fn height(&self) -> u32 {
        64
    }
    fn present_qpc(&self) -> Ticks {
        Ticks::from_raw(1000)
    }
    fn acquire_qpc(&self) -> Ticks {
        Ticks::from_raw(1001)
    }
    fn captured(&self) -> Ticks {
        Ticks::from_raw(1002)
    }
}

struct FakeSource {
    steps: VecDeque<Step>,
    stop: std::rc::Rc<std::cell::Cell<bool>>,
}

impl FrameSource for FakeSource {
    type Frame<'a> = FakeFrame;

    fn next(
        &mut self,
        _timeout: Duration,
    ) -> Result<Option<CaptureEvent<FakeFrame>>, CaptureError> {
        Ok(match self.steps.pop_front() {
            None | Some(Step::Idle) => None,
            Some(Step::Stop) => {
                self.stop.set(true);
                None
            }
            Some(Step::Reopen(w, h)) => Some(CaptureEvent::Reopened {
                width: w,
                height: h,
            }),
            Some(Step::Frame(seq)) => Some(CaptureEvent::Frame(FakeFrame(seq))),
        })
    }
}

/// Records what the loop asked for: ("reopen", w) or ("encode", seq, idr).
#[derive(Default)]
struct FakeSink {
    log: Vec<(&'static str, u64, bool)>,
}

impl EncodeSink<FakeSource> for FakeSink {
    fn reopen(&mut self, _src: &FakeSource, width: u32, _height: u32) -> Result<(), SvcError> {
        self.log.push(("reopen", width.into(), false));
        Ok(())
    }

    fn encode(&mut self, frame: &FakeFrame, force_idr: bool) -> Result<usize, SvcError> {
        self.log.push(("encode", frame.0, force_idr));
        Ok(100)
    }
}

fn run(steps: &[Step]) -> (FakeSink, Stats) {
    let stop = std::rc::Rc::new(std::cell::Cell::new(false));
    let mut src = FakeSource {
        steps: steps.iter().copied().chain([Step::Stop]).collect(),
        stop: stop.clone(),
    };
    let mut sink = FakeSink::default();
    let mut stats = Stats::with_capacity(16);
    let deadline = Instant::now() + Duration::from_secs(10);
    run_loop(&mut src, &mut sink, &|| stop.get(), deadline, &mut stats).unwrap();
    (sink, stats)
}

#[test]
fn first_frame_is_idr() {
    let (sink, stats) = run(&[Step::Reopen(64, 64), Step::Frame(1), Step::Frame(2)]);
    assert_eq!(
        sink.log,
        [
            ("reopen", 64, false),
            ("encode", 1, true),
            ("encode", 2, false)
        ]
    );
    assert_eq!((stats.frames, stats.idrs), (2, 1));
}

#[test]
fn reopen_forces_idr() {
    let (sink, stats) = run(&[
        Step::Reopen(64, 64),
        Step::Frame(1),
        Step::Frame(2),
        Step::Reopen(32, 32),
        Step::Frame(3),
    ]);
    assert_eq!(sink.log[3], ("reopen", 32, false));
    assert_eq!(sink.log[4], ("encode", 3, true));
    assert_eq!((stats.reopens, stats.idrs), (2, 2));
}

#[test]
fn idle_timeout_is_not_an_error() {
    let (sink, stats) = run(&[
        Step::Reopen(64, 64),
        Step::Idle,
        Step::Idle,
        Step::Idle,
        Step::Frame(1),
    ]);
    assert_eq!(sink.log.len(), 2);
    assert_eq!(stats.frames, 1);
    assert_eq!(stats.au_bytes.max(), 100);
}

#[test]
fn stop_ends_loop() {
    let (sink, _) = run(&[Step::Reopen(64, 64), Step::Stop, Step::Frame(1)]);
    assert_eq!(sink.log, [("reopen", 64, false)]);
}

#[test]
fn deadline_ends_loop() {
    let mut src = FakeSource {
        steps: VecDeque::new(),
        stop: std::rc::Rc::new(std::cell::Cell::new(false)),
    };
    let mut sink = FakeSink::default();
    let mut stats = Stats::with_capacity(4);
    run_loop(&mut src, &mut sink, &|| false, Instant::now(), &mut stats).unwrap();
    assert!(sink.log.is_empty());
}
