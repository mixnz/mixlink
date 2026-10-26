//! mDNS registration through the Windows DNS-SD API (spec 0017). The system responder (Dnscache)
//! owns port 5353 and its firewall rule, so we never open a socket for it. Browsing:
//! `dns_sd_browse.rs`. A context Windows may still call back into is leaked on purpose.

use std::ffi::c_void;
use std::mem::ManuallyDrop;
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::time::Duration;

use windows::Win32::Foundation::{DNS_REQUEST_PENDING, HANDLE};
use windows::Win32::NetworkManagement::Dns::{
    DNS_QUERY_REQUEST_VERSION1, DNS_SERVICE_INSTANCE, DNS_SERVICE_REGISTER_REQUEST,
    DnsServiceConstructInstance, DnsServiceDeRegister, DnsServiceFreeInstance, DnsServiceRegister,
};
use windows::Win32::System::SystemInformation::{ComputerNameDnsHostname, GetComputerNameExW};
use windows::core::{BOOL, HSTRING, PCWSTR, PWSTR};

use crate::discovery::advert::{Advert, SERVICE_TYPE};
use crate::discovery::error::DiscoveryError;

const REGISTER_WAIT: Duration = Duration::from_secs(5);
const DEREGISTER_WAIT: Duration = Duration::from_secs(2);

pub(super) fn win32(call: &'static str, code: u32) -> DiscoveryError {
    DiscoveryError::Win32 { call, code }
}

/// # Safety
/// `p` is null or a valid NUL-terminated UTF-16 string.
pub(super) unsafe fn wide(p: PWSTR) -> Option<String> {
    // SAFETY: per the function contract.
    (!p.is_null())
        .then(|| unsafe { p.to_string() }.ok())
        .flatten()
}

fn host_name() -> Result<String, DiscoveryError> {
    let mut len = 0u32;
    // SAFETY: size query with no buffer; `len` is a live local. It fails with "more data" by design.
    let _ = unsafe { GetComputerNameExW(ComputerNameDnsHostname, None, &mut len) };
    let mut buf = vec![0u16; len as usize + 1];
    let mut size = buf.len() as u32;
    // SAFETY: `buf` has `size` writable UTF-16 units; `size` is a live local.
    unsafe {
        GetComputerNameExW(
            ComputerNameDnsHostname,
            Some(PWSTR(buf.as_mut_ptr())),
            &mut size,
        )
    }
    .map_err(|error| win32("GetComputerNameExW", error.code().0 as u32))?;
    buf.truncate(size as usize);
    Ok(format!("{}.local", String::from_utf16_lossy(&buf)))
}

/// Our service registered with the system responder; dropping it sends the mDNS goodbye.
#[derive(Debug)]
pub(crate) struct MdnsRegistration {
    /// Freed only once Windows is done with them (see `Drop`).
    request: ManuallyDrop<Box<DNS_SERVICE_REGISTER_REQUEST>>,
    done: ManuallyDrop<Box<SyncSender<u32>>>,
    events: Receiver<u32>,
}

// SAFETY: the raw pointers inside `request` are only used by `Drop`, from whichever thread owns
// the registration; Windows does not tie them to the registering thread.
unsafe impl Send for MdnsRegistration {}

unsafe extern "system" fn on_registered(
    status: u32,
    ctx: *const c_void,
    instance: *const DNS_SERVICE_INSTANCE,
) {
    if !instance.is_null() {
        // SAFETY: Windows hands the caller this instance to free.
        unsafe { DnsServiceFreeInstance(instance) };
    }
    // SAFETY: `ctx` is the boxed sender owned by `MdnsRegistration`, alive until the callback ran.
    let done = unsafe { &*ctx.cast::<SyncSender<u32>>() };
    let _ = done.try_send(status);
}

impl MdnsRegistration {
    pub(crate) fn register(advert: &Advert) -> Result<MdnsRegistration, DiscoveryError> {
        // Dots would split the instance label; the display name travels in TXT anyway.
        let instance_name =
            HSTRING::from(format!("{}.{SERVICE_TYPE}", advert.name.replace('.', "-")));
        let host = HSTRING::from(host_name()?);
        let txt = advert.txt();
        let keys: Vec<HSTRING> = txt.iter().map(|(k, _)| HSTRING::from(*k)).collect();
        let values: Vec<HSTRING> = txt.iter().map(|(_, v)| HSTRING::from(v.as_str())).collect();
        let key_ptrs: Vec<PCWSTR> = keys.iter().map(|k| PCWSTR(k.as_ptr())).collect();
        let value_ptrs: Vec<PCWSTR> = values.iter().map(|v| PCWSTR(v.as_ptr())).collect();
        // SAFETY: all strings outlive the call, which copies them into the new instance.
        let instance = unsafe {
            DnsServiceConstructInstance(
                &instance_name,
                &host,
                None,
                None,
                advert.port,
                0,
                0,
                key_ptrs.len() as u32,
                key_ptrs.as_ptr(),
                value_ptrs.as_ptr(),
            )
        };
        if instance.is_null() {
            return Err(win32("DnsServiceConstructInstance", 0));
        }
        let (tx, events) = mpsc::sync_channel(2);
        let done = Box::new(tx);
        let request = Box::new(DNS_SERVICE_REGISTER_REQUEST {
            Version: DNS_QUERY_REQUEST_VERSION1.0,
            InterfaceIndex: 0,
            pServiceInstance: instance,
            pRegisterCompletionCallback: Some(on_registered),
            pQueryContext: std::ptr::from_ref::<SyncSender<u32>>(&*done)
                .cast_mut()
                .cast(),
            hCredentials: HANDLE::default(),
            unicastEnabled: BOOL::from(false),
        });
        // SAFETY: `request`, its instance and its context are boxed and kept by the registration.
        let code = unsafe { DnsServiceRegister(&*request, None) };
        if code != DNS_REQUEST_PENDING as u32 {
            // SAFETY: the request was refused, so Windows holds no pointer to the instance.
            unsafe { DnsServiceFreeInstance(instance) };
            return Err(win32("DnsServiceRegister", code));
        }
        let registration = MdnsRegistration {
            request: ManuallyDrop::new(request),
            done: ManuallyDrop::new(done),
            events,
        };
        match registration.events.recv_timeout(REGISTER_WAIT) {
            Ok(0) => Ok(registration),
            Ok(code) => Err(win32("DnsServiceRegister callback", code)),
            Err(_) => Err(win32("DnsServiceRegister callback", 0x5B4)), // ERROR_TIMEOUT
        }
    }
}

impl Drop for MdnsRegistration {
    fn drop(&mut self) {
        // SAFETY: the same request that registered; it stays alive until the callback (or leaks).
        let code = unsafe { DnsServiceDeRegister(&**self.request, None) };
        let finished =
            code == DNS_REQUEST_PENDING as u32 && self.events.recv_timeout(DEREGISTER_WAIT).is_ok();
        if !finished {
            // A late callback would use the request and context: leave both allocated.
            log::warn!("mDNS deregistration did not finish (code {code})");
            return;
        }
        // SAFETY: deregistration completed, so Windows no longer uses the instance, the request or
        // the context; each is freed exactly once here.
        unsafe {
            DnsServiceFreeInstance(self.request.pServiceInstance);
            ManuallyDrop::drop(&mut self.request);
            ManuallyDrop::drop(&mut self.done);
        }
    }
}
