//! `mixlink-stream`: the stream window process spawned by the client launcher.
//! Stdout carries IPC lines only; logs go to stderr.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod connect;
mod ipc;
mod platform;
mod strings;
mod window_input;

use std::io;
use std::process::ExitCode;
use std::thread;

use mixlink_stream::args;
use winit::event_loop::EventLoop;

use crate::app::StreamApp;
use crate::ipc::{StreamEvent, read_commands};

fn main() -> ExitCode {
    env_logger::init();
    // `--connect` from the launcher (spec 0020), `--play` for dev (spec 0013), none for the demo window.
    let mode = match args::parse(std::env::args().skip(1)) {
        Ok(mode) => mode,
        Err(error) => {
            log::error!("arguments: {error}");
            return ExitCode::from(2);
        }
    };
    let event_loop = match EventLoop::<StreamEvent>::with_user_event().build() {
        Ok(event_loop) => event_loop,
        Err(error) => {
            log::error!("event loop: {error}");
            return ExitCode::FAILURE;
        }
    };
    let proxy = event_loop.create_proxy();
    let stdin_proxy = proxy.clone();
    let reader = thread::Builder::new()
        .name("ipc-stdin".into())
        .spawn(move || {
            read_commands(io::stdin().lock(), |event| {
                // Fails only after the loop has exited; nothing left to notify.
                let _ = stdin_proxy.send_event(event);
            });
        });
    if let Err(error) = reader {
        log::error!("stdin reader thread: {error}");
    }
    let mut app = StreamApp::new(proxy, mode);
    match event_loop.run_app(&mut app) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            log::error!("event loop: {error}");
            ExitCode::FAILURE
        }
    }
}
