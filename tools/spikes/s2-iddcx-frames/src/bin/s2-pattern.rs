//! Test pattern on the virtual monitor: new background color every frame and the frame counter
//! in the top-left cell (see `counter.rs`). Logs presents and vsync glitches per second.
//! Usage: `s2-pattern [--minutes 12]`. Esc or closing the window stops it.

use std::fs::File;
use std::io::{BufWriter, Write};
use std::sync::Arc;

use s2_iddcx_frames::counter::{BLOCK, encode};
use s2_iddcx_frames::display::virtual_gdi_name;
use s2_iddcx_frames::frames::vsync_glitches;
use s2_iddcx_frames::pattern_gfx::{Gfx, create_gfx};
use s2_iddcx_frames::qpc;
use s2_iddcx_frames::util::{BoxResult, arg_u64, now_stamp, out_dir};
use windows::Win32::Graphics::Direct3D11::D3D11_BOX;
use windows::Win32::Graphics::Dxgi::{DXGI_FRAME_STATISTICS, DXGI_PRESENT};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Fullscreen, Window, WindowId};

struct App {
    deadline: u64,
    window: Option<Arc<Window>>,
    gfx: Option<Gfx>,
    csv: BufWriter<File>,
    frame: u32,
    last_stats: Option<(u32, u32)>,
    sec_start: u64,
    presents: u32,
    glitches: u32,
    error: Option<String>,
}

impl App {
    fn render(&mut self) -> BoxResult<()> {
        let Some(gfx) = &self.gfx else { return Ok(()) };
        self.frame = self.frame.wrapping_add(1);
        let hue = (self.frame % 360) as f32 / 360.0;
        let color = [hue, 1.0 - hue, 0.5, 1.0];
        let src_box = D3D11_BOX {
            left: 0,
            top: 0,
            front: 0,
            right: BLOCK,
            bottom: BLOCK,
            back: 1,
        };
        // SAFETY: all resources belong to this device and are alive.
        unsafe {
            gfx.context.ClearRenderTargetView(&gfx.rtv, &color);
            for (i, bit) in encode(self.frame).iter().enumerate() {
                let src = if *bit { &gfx.white } else { &gfx.black };
                gfx.context.CopySubresourceRegion(
                    &gfx.back,
                    0,
                    i as u32 * BLOCK,
                    0,
                    0,
                    src,
                    0,
                    Some(&src_box),
                );
            }
            gfx.swapchain.Present(1, DXGI_PRESENT(0)).ok()?;
        }
        self.presents += 1;
        let mut stats = DXGI_FRAME_STATISTICS::default();
        // SAFETY: valid out-pointer. Fails until the first frame is shown; that is not an error.
        if unsafe { gfx.swapchain.GetFrameStatistics(&mut stats) }.is_ok() {
            let cur = (stats.PresentCount, stats.PresentRefreshCount);
            if let Some(prev) = self.last_stats {
                self.glitches += vsync_glitches(prev, cur);
            }
            self.last_stats = Some(cur);
        }
        let now = qpc::now();
        if now - self.sec_start >= qpc::freq() {
            writeln!(self.csv, "{now},{},{}", self.presents, self.glitches)?;
            self.csv.flush()?;
            (self.sec_start, self.presents, self.glitches) = (now, 0, 0);
        }
        Ok(())
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let target = virtual_gdi_name();
        let monitor = event_loop.available_monitors().find(|m| m.name() == target);
        let Some(monitor) = monitor else {
            self.error = Some(format!("virtual monitor not found (looked for {target:?})"));
            event_loop.exit();
            return;
        };
        let attrs = Window::default_attributes()
            .with_title("MixLink S2 pattern")
            .with_fullscreen(Some(Fullscreen::Borderless(Some(monitor))));
        let result = event_loop
            .create_window(attrs)
            .map_err(|e| e.into())
            .and_then(|w| {
                let w = Arc::new(w);
                let gfx = create_gfx(&w)?;
                Ok::<_, Box<dyn std::error::Error>>((w, gfx))
            });
        match result {
            Ok((w, gfx)) => {
                self.window = Some(w);
                self.gfx = Some(gfx);
            }
            Err(e) => {
                self.error = Some(e.to_string());
                event_loop.exit();
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested
            | WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        logical_key: Key::Named(NamedKey::Escape),
                        state: ElementState::Pressed,
                        ..
                    },
                ..
            } => event_loop.exit(),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if qpc::now() >= self.deadline {
            event_loop.exit();
            return;
        }
        if let Err(e) = self.render() {
            self.error = Some(e.to_string());
            event_loop.exit();
        }
    }
}

fn main() -> BoxResult<()> {
    let args: Vec<String> = std::env::args().collect();
    let minutes = arg_u64(&args, "--minutes", 12)?;
    let path = out_dir()?.join(format!("{}-pattern.csv", now_stamp()));
    let mut csv = BufWriter::new(File::create(&path)?);
    writeln!(csv, "t_qpc,presents,glitches")?;
    let start = qpc::now();
    let mut app = App {
        deadline: start + minutes * 60 * qpc::freq(),
        window: None,
        gfx: None,
        csv,
        frame: 0,
        last_stats: None,
        sec_start: start,
        presents: 0,
        glitches: 0,
        error: None,
    };
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);
    event_loop.run_app(&mut app)?;
    eprintln!("s2-pattern: {} frames, log {}", app.frame, path.display());
    match app.error {
        Some(e) => Err(e.into()),
        None => Ok(()),
    }
}
