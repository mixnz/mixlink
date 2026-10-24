//! `net-probe`: dev tool for the UDP transport (spec 0011) on loopback or between two PCs.

mod link;
mod run;
mod serve;
mod wire;

use std::net::Ipv4Addr;
use std::process::ExitCode;

use mixlink_core::config::NetConfig;

use crate::run::RunOptions;

const USAGE: &str = "usage:
  net-probe serve [--port P]
  net-probe run <host-ip> [--port P] [--seconds 30] [--mbps 50] [--fps 120]";

#[derive(Debug)]
enum Command {
    Serve { port: u16 },
    Run(RunOptions),
}

fn main() -> ExitCode {
    env_logger::init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = match parse(&args) {
        Ok(command) => command,
        Err(problem) => {
            eprintln!("{problem}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    let result = match command {
        Command::Serve { port } => serve::serve(port),
        Command::Run(opts) => run::run(opts),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("net-probe: {error}");
            ExitCode::FAILURE
        }
    }
}

fn parse(args: &[String]) -> Result<Command, String> {
    let (mode, rest) = args.split_first().ok_or("missing command")?;
    let (host, flags) = match mode.as_str() {
        "serve" => (None, rest),
        "run" => {
            let (ip, flags) = rest.split_first().ok_or("missing host ip")?;
            let ip: Ipv4Addr = ip.parse().map_err(|_| format!("bad IPv4 address: {ip}"))?;
            (Some(ip), flags)
        }
        other => return Err(format!("unknown command: {other}")),
    };
    let mut opts = RunOptions {
        host: Ipv4Addr::UNSPECIFIED,
        port: NetConfig::default().session_port,
        seconds: 30,
        mbps: 50,
        fps: 120,
    };
    let mut flags = flags.iter();
    while let Some(flag) = flags.next() {
        let value = flags.next().ok_or(format!("{flag} needs a value"))?;
        let number = |min: u64| -> Result<u64, String> {
            match value.parse::<u64>() {
                Ok(n) if n >= min => Ok(n),
                _ => Err(format!("bad value for {flag}: {value}")),
            }
        };
        match flag.as_str() {
            "--port" => {
                opts.port = u16::try_from(number(1)?).map_err(|_| format!("bad port: {value}"))?;
            }
            "--seconds" if host.is_some() => opts.seconds = number(1)?,
            "--mbps" if host.is_some() => opts.mbps = number(1)?,
            "--fps" if host.is_some() => opts.fps = number(1)?,
            _ => return Err(format!("unknown option: {flag}")),
        }
    }
    Ok(match host {
        None => Command::Serve { port: opts.port },
        Some(ip) => Command::Run(RunOptions { host: ip, ..opts }),
    })
}
