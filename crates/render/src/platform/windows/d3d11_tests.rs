use std::num::NonZeroIsize;
use std::sync::Arc;

use mixlink_core::video::ColorMatrix;
use raw_window_handle::{
    HandleError, HasWindowHandle, RawWindowHandle, WebWindowHandle, Win32WindowHandle, WindowHandle,
};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, WINDOW_EX_STYLE, WS_OVERLAPPEDWINDOW,
};
use windows::core::w;

use crate::platform::windows::test_util::nv12_bands;
use crate::{Presenter, RenderError, VideoFrame};

/// Hidden top-level window owned by a test.
struct TestWindow(HWND);

// SAFETY: the HWND is only used as an opaque handle for the swapchain; no thread affinity is relied on.
unsafe impl Send for TestWindow {}
// SAFETY: see `Send`; the handle is never mutated after creation.
unsafe impl Sync for TestWindow {}

impl TestWindow {
    fn new() -> Arc<Self> {
        // SAFETY: "STATIC" is a system window class; all pointer args are null/None.
        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("STATIC"),
                w!("mixlink-render-test"),
                WS_OVERLAPPEDWINDOW,
                0,
                0,
                64,
                64,
                None,
                None,
                None,
                None,
            )
        }
        .unwrap();
        Arc::new(Self(hwnd))
    }
}

impl Drop for TestWindow {
    fn drop(&mut self) {
        // SAFETY: the window was created by this test and is destroyed exactly once.
        let _ = unsafe { DestroyWindow(self.0) };
    }
}

impl HasWindowHandle for TestWindow {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        let hwnd = NonZeroIsize::new(self.0.0 as isize).ok_or(HandleError::Unavailable)?;
        let raw = RawWindowHandle::Win32(Win32WindowHandle::new(hwnd));
        // SAFETY: the HWND stays valid while `self` is borrowed (destroyed only on drop).
        Ok(unsafe { WindowHandle::borrow_raw(raw) })
    }
}

/// Window that reports a non-Win32 handle.
struct WebWindow;

impl HasWindowHandle for WebWindow {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        let raw = RawWindowHandle::Web(WebWindowHandle::new(1));
        // SAFETY: a Web handle is a plain id; nothing dereferences it.
        Ok(unsafe { WindowHandle::borrow_raw(raw) })
    }
}

#[test]
fn presents_on_hidden_window() {
    let mut presenter = Presenter::new(TestWindow::new(), 64, 64).unwrap();
    presenter.clear_and_present([0.0, 0.0, 0.0, 1.0]).unwrap();
}

#[test]
fn resizes_and_presents() {
    let mut presenter = Presenter::new(TestWindow::new(), 64, 64).unwrap();
    presenter.resize(128, 96).unwrap();
    presenter.clear_and_present([0.2, 0.4, 0.6, 1.0]).unwrap();
}

#[test]
fn minimized_skips_present() {
    let mut presenter = Presenter::new(TestWindow::new(), 64, 64).unwrap();
    presenter.resize(0, 0).unwrap();
    presenter.clear_and_present([1.0, 0.0, 0.0, 1.0]).unwrap();
    presenter.resize(64, 64).unwrap();
    presenter.clear_and_present([1.0, 0.0, 0.0, 1.0]).unwrap();
}

#[test]
fn non_win32_handle_is_rejected() {
    let result = Presenter::new(Arc::new(WebWindow), 64, 64);
    assert!(matches!(result, Err(RenderError::UnsupportedWindow)));
}

fn white_frame_test(presenter: &mut Presenter, size: (u32, u32)) {
    let device = presenter.device();
    let texture = nv12_bands(&device, 64, 64, &[(235, 128, 128)]);
    let frame = VideoFrame {
        texture: &texture,
        array_index: 0,
        width: 64,
        height: 64,
        matrix: ColorMatrix::Bt709,
    };
    presenter.resize(size.0, size.1).unwrap();
    presenter.present_nv12(&frame).unwrap();
    presenter.present_nv12(&frame).unwrap();
}

#[test]
fn presents_nv12_frame() {
    let mut presenter = Presenter::new(TestWindow::new(), 64, 64).unwrap();
    white_frame_test(&mut presenter, (96, 64));
}

#[test]
fn present_nv12_when_minimized_is_ok() {
    let mut presenter = Presenter::new(TestWindow::new(), 64, 64).unwrap();
    white_frame_test(&mut presenter, (0, 0));
    white_frame_test(&mut presenter, (64, 64));
}

#[test]
fn wait_gpu_returns() {
    let mut presenter = Presenter::new(TestWindow::new(), 64, 64).unwrap();
    white_frame_test(&mut presenter, (64, 64));
    presenter.wait_gpu();
}
