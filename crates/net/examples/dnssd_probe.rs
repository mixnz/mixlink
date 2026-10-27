//! Manual mDNS checks for spec 0017 → Results.
//! `register <name> [seconds]`: advertise over mDNS only, until Enter (or for `seconds`).
//! `browse`: list hosts seen over mDNS only, printing every change with a timestamp.

use std::io::BufRead;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use mixlink_core::ids::DeviceId;
use mixlink_net::discovery::{
    Advert, Advertiser, AdvertiserConfig, Browser, BrowserConfig, CAPS_V1, ProbeTargets,
};

fn main() {
    env_logger::init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("register") => register(args.get(1).map_or("Probe", String::as_str), args.get(2)),
        Some("browse") => browse(),
        _ => Err("usage: dnssd_probe register <name> [seconds] | browse".to_owned()),
    };
    if let Err(error) = result {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn register(name: &str, seconds: Option<&String>) -> Result<(), String> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let advert = Advert {
        id: DeviceId::from_bytes(nanos.to_le_bytes()),
        name: name.to_owned(),
        ver: 1,
        caps: CAPS_V1.to_owned(),
        port: 47801,
    };
    let cfg = AdvertiserConfig {
        discovery_port: 0,
        mdns: true,
    };
    let advertiser = Advertiser::start(advert.clone(), cfg).map_err(|e| e.to_string())?;
    eprintln!("registered {} ({})", advert.name, advert.id);
    match seconds.map(|s| s.parse::<u64>()) {
        Some(Ok(secs)) => std::thread::sleep(Duration::from_secs(secs)),
        Some(Err(error)) => return Err(error.to_string()),
        None => {
            eprintln!("press Enter to deregister");
            let _ = std::io::stdin().lock().lines().next();
        }
    }
    drop(advertiser);
    eprintln!("deregistered");
    Ok(())
}

fn browse() -> Result<(), String> {
    let started = Instant::now();
    let cfg = BrowserConfig {
        targets: ProbeTargets::Fixed(Vec::new()),
        ..BrowserConfig::default()
    };
    let browser = Browser::start(
        cfg,
        Box::new(move |hosts| {
            let at = started.elapsed().as_secs_f32();
            eprintln!("[{at:7.2}s] {} host(s)", hosts.len());
            for host in hosts {
                eprintln!(
                    "    {} {} {}:{}",
                    host.id, host.name, host.address, host.port
                );
            }
        }),
    )
    .map_err(|e| e.to_string())?;
    eprintln!("browsing mDNS; press Enter to stop");
    let _ = std::io::stdin().lock().lines().next();
    drop(browser);
    Ok(())
}
