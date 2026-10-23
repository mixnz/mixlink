//! `s4-ctl check`: adapters, FFmpeg build, encoders. `verify <file>`: decode a dump. `pool <case> <job>`:
//! pooled stats per side over the round CSVs in `out/`.

use std::path::Path;

use s4_encode_audio::compare;
use s4_encode_audio::platform::{d3d, ff_probe, ff_verify};
use s4_encode_audio::util::{BoxResult, now_stamp, out_dir};

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

/// Pools every round CSV of `case` / `job` in `out/` per side; prints and saves the result.
fn pool(case: &str, job: &str) -> BoxResult<()> {
    let dir = out_dir()?;
    let names: Vec<String> = std::fs::read_dir(&dir)?
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .collect();
    let mut sides = Vec::new();
    let mut listing = String::new();
    for (side, files) in compare::side_files(&names, case, job) {
        let mut rounds = Vec::new();
        for f in &files {
            rounds.push(compare::parse_samples_csv(&std::fs::read_to_string(
                dir.join(f),
            )?)?);
            listing.push_str(&format!("  {side}: {f}\n"));
        }
        sides.push((side, rounds));
    }
    if sides.is_empty() {
        return Err(format!("no {job} CSVs for case {case} in {}", dir.display()).into());
    }
    let text = format!(
        "pool case {case} job {job}\n{listing}{}",
        compare::pool(&sides)
    );
    std::fs::write(
        dir.join(format!("{}-{case}-pool-{job}.txt", now_stamp())),
        &text,
    )?;
    print!("{text}");
    Ok(())
}

fn main() -> BoxResult<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let arg = |i: usize| args.get(i).map(String::as_str);
    match (arg(0), arg(1), arg(2)) {
        (Some("check"), _, _) => check(),
        (Some("verify"), Some(path), _) => verify(path),
        (Some("pool"), Some(case), Some(job)) => pool(case, job),
        _ => Err(
            "usage: s4-ctl check | verify <file.h264|file.hevc> | pool <case> <encode|audio>"
                .into(),
        ),
    }
}
