use std::sync::Arc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use mixlink_codec::VideoCodec;
use winit::raw_window_handle::{HandleError, HasWindowHandle, WindowHandle};

use super::{VideoEvent, VideoSource, VideoThread};
use crate::au_queue::channel;

/// A window whose handle is never available: the presenter cannot be made.
struct NoWindow;

impl HasWindowHandle for NoWindow {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        Err(HandleError::Unavailable)
    }
}

#[test]
fn video_failure_closes_cleanly() {
    let (events_tx, events_rx) = mpsc::channel();
    let (_au_tx, au_rx) = channel(2, 16);
    let source = VideoSource {
        rx: au_rx,
        codec: VideoCodec::H264,
        measure: false,
    };
    let video = VideoThread::start(Arc::new(NoWindow), (64, 64), Some(source), move |e| {
        let _ = events_tx.send(e);
    })
    .unwrap();
    let event = events_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(matches!(event, VideoEvent::Failed(_)), "{event:?}");
    let started = Instant::now();
    let _stats = video.stop();
    assert!(started.elapsed() < Duration::from_millis(100));
}
