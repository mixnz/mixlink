//! `mixlink-stream`: the stream window process spawned by the client launcher.
//! Stdout carries IPC lines only; logs go to stderr.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod ipc;
mod platform;
mod strings;

use std::io;
use std::process::ExitCode;
use std::thread;

use mixlink_stream::args;
use winit::event_loop::EventLoop;

use crate::app::StreamApp;
use crate::ipc::{StreamEvent, read_commands};

fn main() -> ExitCode {
    env_logger::init();
    // Dev play mode only (spec 0013); the launcher passes no arguments.
    let play = match args::parse(std::env::args().skip(1)) {
        Ok(play) => play,
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
    let mut app = StreamApp::new(proxy, play);
    match event_loop.run_app(&mut app) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            log::error!("event loop: {error}");
            ExitCode::FAILURE
        }
    }
}
