//! `s4-ctl check`: adapters, FFmpeg build and encoders. `s4-ctl verify <file>`: decode a dump.

use std::path::Path;

use s4_encode_audio::platform::{d3d, ff_probe, ff_verify};
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

fn verify(path: &str) -> BoxResult<()> {
    let v = ff_verify::verify(Path::new(path))?;
    let numbers: Vec<String> = v.numbers.iter().map(|(i, n)| format!("{i}->{n}")).collect();
    let ok = v.errors == 0 && v.numbers.iter().all(|(i, n)| i == n);
    println!(
        "{path}: frames {} errors {} numbers [{}] -> {}",
        v.frames,
        v.errors,
        numbers.join(" "),
        if ok { "OK" } else { "FAIL" }
    );
    Ok(())
}

fn main() -> BoxResult<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match (args.first().map(String::as_str), args.get(1)) {
        (Some("check"), _) => check(),
        (Some("verify"), Some(path)) => verify(path),
        _ => Err("usage: s4-ctl check | verify <file.h264|file.hevc>".into()),
    }
}
