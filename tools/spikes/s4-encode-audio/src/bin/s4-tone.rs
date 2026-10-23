//! `s4-tone [--minutes N]`: 1 kHz test tone at -12 dBFS on the default output (user session), for the
//! loopback cases. Default 30 minutes; Ctrl+C stops it earlier.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use s4_encode_audio::platform::tone;
use s4_encode_audio::util::{BoxResult, arg_u64};

fn main() -> BoxResult<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let minutes = arg_u64(&args, "--minutes", 30)?;
    let stop = AtomicBool::new(false);
    println!("Playing 1 kHz at -12 dBFS for {minutes} min on the default output. Ctrl+C stops.");
    std::thread::scope(|s| {
        let player = s.spawn(|| tone::play(&stop).map_err(|e| e.to_string()));
        std::thread::sleep(Duration::from_secs(minutes * 60));
        stop.store(true, Ordering::Release);
        Ok(player.join().map_err(|_| "tone thread panicked")??)
    })
}
