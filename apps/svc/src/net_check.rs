//! Network checks for LAN discovery (spec 0017): the firewall rule's ports and a cached
//! "is a connected network Public?" answer for the tray.

use std::time::{Duration, Instant};

use mixlink_core::config::NetConfig;

/// How long a Public-network answer is reused; the tray asks every second.
pub const PUBLIC_CHECK_TTL: Duration = Duration::from_secs(5);

/// Firewall local ports: `"<discovery>,<session>"`, one port when both are the same.
pub fn local_ports(net: &NetConfig) -> String {
    if net.discovery_port == net.session_port {
        net.discovery_port.to_string()
    } else {
        format!("{},{}", net.discovery_port, net.session_port)
    }
}

#[derive(Debug)]
pub struct PublicCheck {
    ttl: Duration,
    last: Option<(Instant, bool)>,
}

impl PublicCheck {
    pub fn new(ttl: Duration) -> PublicCheck {
        PublicCheck { ttl, last: None }
    }

    /// The cached answer, or a fresh one from `probe` once it is `ttl` old.
    pub fn get(&mut self, now: Instant, probe: impl FnOnce() -> bool) -> bool {
        match self.last {
            Some((at, answer)) if now.saturating_duration_since(at) < self.ttl => answer,
            _ => {
                let answer = probe();
                self.last = Some((now, answer));
                answer
            }
        }
    }
}

#[cfg(test)]
#[path = "net_check_tests.rs"]
mod tests;
