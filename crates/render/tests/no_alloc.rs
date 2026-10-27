//! Hot-path check: `Presenter::present_nv12` never allocates on the Rust heap once warm.
//! Test-only counting allocator (spec 0013); it delegates every call to `System`.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::num::NonZeroIsize;
use std::sync::Arc;

use mixlink_core::video::ColorMatrix;
use mixlink_render::{Presenter, VideoFrame};
use raw_window_handle::{
    HandleError, HasWindowHandle, RawWindowHandle, Win32WindowHandle, WindowHandle,
};
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Direct3D11::{
    D3D11_BIND_SHADER_RESOURCE, D3D11_SUBRESOURCE_DATA, D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT,
    ID3D11Texture2D,
};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_NV12, DXGI_SAMPLE_DESC};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, WINDOW_EX_STYLE, WS_OVERLAPPEDWINDOW,
};
use windows::core::w;

thread_local! {
    // `const` init and no destructor: reading it never allocates, so the allocator cannot recurse.
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}

struct CountingAlloc;

// SAFETY: every method forwards to `System` unchanged; the counter only adds a TLS increment.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.with(|c| c.set(c.get() + 1));
        // SAFETY: caller upholds `GlobalAlloc::alloc`'s contract, passed through as is.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: `ptr` came from `System.alloc` above with this `layout`.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOCATIONS.with(|c| c.set(c.get() + 1));
        // SAFETY: `ptr` came from `System` with `layout`; caller upholds the rest.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: CountingAlloc = CountingAlloc;

fn allocations_during(f: impl FnOnce()) -> usize {
    let before = ALLOCATIONS.with(Cell::get);
    f();
    ALLOCATIONS.with(Cell::get) - before
}

/// Hidden top-level window owned by the test.
struct TestWindow(HWND);

// SAFETY: the HWND is only an opaque handle for the swapchain.
unsafe impl Send for TestWindow {}
// SAFETY: see `Send`; never mutated after creation.
unsafe impl Sync for TestWindow {}

impl Drop for TestWindow {
    fn drop(&mut self) {
        // SAFETY: created by this test, destroyed once.
        let _ = unsafe { DestroyWindow(self.0) };
    }
}

impl HasWindowHandle for TestWindow {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        let hwnd = NonZeroIsize::new(self.0.0 as isize).ok_or(HandleError::Unavailable)?;
        let raw = RawWindowHandle::Win32(Win32WindowHandle::new(hwnd));
        // SAFETY: the HWND stays valid while `self` is borrowed.
        Ok(unsafe { WindowHandle::borrow_raw(raw) })
    }
}

fn window() -> Option<Arc<TestWindow>> {
    // SAFETY: "STATIC" is a system window class; all pointer args are null/None.
    let hwnd = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("STATIC"),
            w!("mixlink-render-no-alloc"),
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
    .ok()?;
    Some(Arc::new(TestWindow(hwnd)))
}

fn nv12_grey(presenter: &Presenter) -> Option<ID3D11Texture2D> {
    let bytes = [126u8; 64 * 64 * 3 / 2];
    let desc = D3D11_TEXTURE2D_DESC {
        Width: 64,
        Height: 64,
        MipLevels: 1,
        ArraySize: 1,
        Format: DXGI_FORMAT_NV12,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Usage: D3D11_USAGE_DEFAULT,
        BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
        ..Default::default()
    };
    let data = D3D11_SUBRESOURCE_DATA {
        pSysMem: bytes.as_ptr().cast(),
        SysMemPitch: 64,
        SysMemSlicePitch: 0,
    };
    let mut texture = None;
    // SAFETY: `desc` and `data` (pointing into `bytes`) outlive the call; valid out-pointer.
    unsafe {
        presenter
            .device()
            .CreateTexture2D(&desc, Some(&data), Some(&mut texture))
    }
    .ok()?;
    texture
}

#[test]
fn counter_sees_allocations() {
    let n = allocations_during(|| drop(std::hint::black_box(vec![1u8; 64])));
    assert!(n >= 1);
}

#[test]
fn present_nv12_does_not_allocate_when_warm() {
    let mut presenter = Presenter::new(window().unwrap(), 64, 64).unwrap();
    let texture = nv12_grey(&presenter).unwrap();
    let frame = VideoFrame {
        texture: &texture,
        array_index: 0,
        width: 64,
        height: 64,
        matrix: ColorMatrix::Bt709,
    };
    presenter.present_nv12(&frame).unwrap();
    presenter.present_nv12(&frame).unwrap();
    let mut failed = 0;
    let n = allocations_during(|| {
        for _ in 0..100 {
            failed += usize::from(presenter.present_nv12(&frame).is_err());
        }
    });
    assert_eq!(failed, 0);
    assert_eq!(n, 0, "present_nv12 allocated {n} times");
}
