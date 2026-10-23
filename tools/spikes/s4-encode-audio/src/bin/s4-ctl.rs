//! `s4-ctl check`: adapters, FFmpeg build and encoders (later: audio endpoints, verify, pool).

use s4_encode_audio::platform::{d3d, ff_probe};
use s4_encode_audio::util::BoxResult;

fn check() -> BoxResult<()> {
    println!("adapters:");
    for (name, vendor, mb) in d3d::adapters()? {
        println!("  {name} (vendor {vendor:#06x}, {mb} MB)");
    }
    println!("{}", ff_probe::version());
    println!("encoders:");
    for (name, found) in ff_probe::encoders() {
        println!("  {name}: {}", if found { "found" } else { "missing" });
    }
    Ok(())
}

fn main() -> BoxResult<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("check") => check(),
        _ => Err("usage: s4-ctl check".into()),
    }
}
