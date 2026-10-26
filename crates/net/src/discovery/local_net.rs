//! One IPv4 address of a local interface with its on-link prefix.

use std::net::Ipv4Addr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalNet {
    pub addr: Ipv4Addr,
    pub prefix: u8,
}

impl LocalNet {
    fn mask(&self) -> u32 {
        match self.prefix {
            0 => 0,
            p => u32::MAX << (32 - u32::from(p.min(32))),
        }
    }

    pub fn contains(&self, ip: Ipv4Addr) -> bool {
        (u32::from(self.addr) ^ u32::from(ip)) & self.mask() == 0
    }

    /// Directed broadcast address; `None` for /31 and /32, which have none.
    pub fn broadcast(&self) -> Option<Ipv4Addr> {
        (self.prefix < 31).then(|| Ipv4Addr::from(u32::from(self.addr) | !self.mask()))
    }
}

#[cfg(test)]
#[path = "local_net_tests.rs"]
mod tests;
