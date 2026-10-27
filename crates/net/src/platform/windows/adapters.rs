//! IPv4 addresses of the interfaces that are up, for broadcast probes (spec 0017).

use std::net::Ipv4Addr;

use windows::Win32::Foundation::{ERROR_BUFFER_OVERFLOW, ERROR_NO_DATA};
use windows::Win32::NetworkManagement::IpHelper::{
    GAA_FLAG_SKIP_ANYCAST, GAA_FLAG_SKIP_DNS_SERVER, GAA_FLAG_SKIP_MULTICAST, GetAdaptersAddresses,
    IP_ADAPTER_ADDRESSES_LH,
};
use windows::Win32::NetworkManagement::Ndis::IfOperStatusUp;
use windows::Win32::Networking::WinSock::{AF_INET, SOCKADDR_IN};

use crate::discovery::error::DiscoveryError;
use crate::discovery::local_net::LocalNet;

/// The adapter list usually fits; Windows says how much it needs when it does not.
const FIRST_GUESS: u32 = 16 * 1024;
const ATTEMPTS: usize = 3;

pub(crate) fn local_ipv4_nets() -> Result<Vec<LocalNet>, DiscoveryError> {
    let flags = GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST | GAA_FLAG_SKIP_DNS_SERVER;
    let mut size = FIRST_GUESS;
    for _ in 0..ATTEMPTS {
        // u64 elements keep the buffer aligned for the adapter structs.
        let mut buf = vec![0u64; size.div_ceil(8) as usize];
        let first = buf.as_mut_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>();
        // SAFETY: `first` points to `size` writable bytes owned by `buf`; `size` is a live local.
        let code = unsafe {
            GetAdaptersAddresses(u32::from(AF_INET.0), flags, None, Some(first), &mut size)
        };
        match code {
            0 => {
                // SAFETY: on success `buf` holds a linked list Windows built inside it; `buf` is
                // not touched until the walk returns.
                return Ok(unsafe { walk(first) });
            }
            c if c == ERROR_BUFFER_OVERFLOW.0 => continue,
            c if c == ERROR_NO_DATA.0 => return Ok(Vec::new()),
            c => {
                return Err(DiscoveryError::Win32 {
                    call: "GetAdaptersAddresses",
                    code: c,
                });
            }
        }
    }
    Err(DiscoveryError::Win32 {
        call: "GetAdaptersAddresses",
        code: ERROR_BUFFER_OVERFLOW.0,
    })
}

/// # Safety
/// `first` is the head of an adapter list filled by `GetAdaptersAddresses`, still alive.
unsafe fn walk(first: *const IP_ADAPTER_ADDRESSES_LH) -> Vec<LocalNet> {
    let mut nets = Vec::new();
    let mut adapter = first;
    while !adapter.is_null() {
        // SAFETY: non-null nodes of the list are valid per the function contract.
        let a = unsafe { &*adapter };
        if a.OperStatus == IfOperStatusUp {
            let mut unicast = a.FirstUnicastAddress;
            while !unicast.is_null() {
                // SAFETY: same list, same lifetime.
                let u = unsafe { &*unicast };
                let sockaddr = u.Address.lpSockaddr;
                // SAFETY: `lpSockaddr` points to a socket address of at least the family field;
                // AF_INET entries are `SOCKADDR_IN`, and the address union is plain bytes.
                let ip = unsafe {
                    (!sockaddr.is_null() && (*sockaddr).sa_family == AF_INET).then(|| {
                        let sin = &*sockaddr.cast::<SOCKADDR_IN>();
                        Ipv4Addr::from(sin.sin_addr.S_un.S_addr.to_ne_bytes())
                    })
                };
                if let Some(addr) = ip.filter(|ip| !ip.is_loopback()) {
                    nets.push(LocalNet {
                        addr,
                        prefix: u.OnLinkPrefixLength,
                    });
                }
                unicast = u.Next;
            }
        }
        adapter = a.Next;
    }
    nets
}
