//! mDNS browsing through the Windows DNS-SD API (spec 0017): browse `_mixlink._udp.local`, resolve
//! each instance for TXT + address, report sightings and goodbyes as `DiscoveryEvent`s.
//!
//! Windows may still run a callback after a cancel returns, so `Shared` is leaked for good and a
//! resolve slot whose callback has not run is leaked on drop (a few hundred bytes each).

use std::collections::HashMap;
use std::ffi::c_void;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Mutex, MutexGuard, PoisonError};

use mixlink_core::ids::DeviceId;
use windows::Win32::Foundation::DNS_REQUEST_PENDING;
use windows::Win32::NetworkManagement::Dns::{
    DNS_QUERY_REQUEST_VERSION1, DNS_RECORDW, DNS_SERVICE_BROWSE_REQUEST,
    DNS_SERVICE_BROWSE_REQUEST_0, DNS_SERVICE_CANCEL, DNS_SERVICE_INSTANCE,
    DNS_SERVICE_RESOLVE_REQUEST, DNS_TYPE_PTR, DnsFree, DnsFreeRecordList, DnsServiceBrowse,
    DnsServiceBrowseCancel, DnsServiceFreeInstance, DnsServiceResolve, DnsServiceResolveCancel,
};
use windows::core::{HSTRING, PCWSTR, PWSTR};

use super::dns_sd::{wide, win32};
use crate::discovery::advert::{Advert, SERVICE_TYPE};
use crate::discovery::browser::DiscoveryEvent;
use crate::discovery::error::DiscoveryError;
use crate::discovery::list::Source;

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// State shared with the callbacks; never freed.
#[derive(Debug)]
struct Shared {
    closed: AtomicBool,
    tx: Mutex<Option<Sender<DiscoveryEvent>>>,
    /// Instance name (lowercase) → host id, from resolves; used for goodbyes.
    ids: Mutex<HashMap<String, DeviceId>>,
    /// Instances announced since the last `poll`.
    found: Mutex<Vec<String>>,
    /// Instances (lowercase) that said goodbye since the last `poll`.
    lost: Mutex<Vec<String>>,
}

impl Shared {
    fn send(&self, event: DiscoveryEvent) {
        if self.closed.load(Ordering::Acquire) {
            return;
        }
        if let Some(tx) = lock(&self.tx).as_ref() {
            let _ = tx.send(event);
        }
    }

    fn goodbye(&self, name: &str) {
        let key = name.to_lowercase();
        lock(&self.lost).push(key.clone());
        if let Some(id) = lock(&self.ids).remove(&key) {
            self.send(DiscoveryEvent::Gone(id, Source::Mdns));
        }
    }
}

/// Context of one resolve: the query name must outlive the request.
#[derive(Debug)]
struct ResolveSlot {
    shared: &'static Shared,
    name: Vec<u16>,
    done: AtomicBool,
}

/// A running browse for MixLink hosts; `poll` it often from the owning thread.
#[derive(Debug)]
pub(crate) struct MdnsBrowse {
    shared: &'static Shared,
    cancel: Box<DNS_SERVICE_CANCEL>,
    /// Lowercase instance name → name as announced.
    known: HashMap<String, String>,
    /// Lowercase instance name → its pending resolve.
    slots: HashMap<String, (Box<ResolveSlot>, Box<DNS_SERVICE_CANCEL>)>,
}

// SAFETY: cancel handles and slots are touched only by the owning thread (`poll`, `Drop`);
// callbacks reach `Shared` (mutexes, atomics) and a slot's atomic `done` flag only.
unsafe impl Send for MdnsBrowse {}

unsafe extern "system" fn on_browse(_status: u32, ctx: *const c_void, records: *const DNS_RECORDW) {
    // SAFETY: `ctx` is the leaked, never-freed `Shared` of the browse.
    let shared = unsafe { &*ctx.cast::<Shared>() };
    let mut record = records;
    while !record.is_null() {
        // SAFETY: a node of the record list Windows passed in; freed below, after the walk.
        let r = unsafe { &*record };
        if r.wType == DNS_TYPE_PTR.0 {
            // SAFETY: for PTR records the `PTR` union member is the active one.
            if let Some(name) = unsafe { wide(r.Data.PTR.pNameHost) } {
                if r.dwTtl == 0 {
                    shared.goodbye(&name);
                } else {
                    lock(&shared.found).push(name);
                }
            }
        }
        record = r.pNext;
    }
    if !records.is_null() {
        // SAFETY: the list belongs to the callback, which frees it once.
        unsafe { DnsFree(Some(records.cast()), DnsFreeRecordList) };
    }
}

unsafe extern "system" fn on_resolved(
    status: u32,
    ctx: *const c_void,
    instance: *const DNS_SERVICE_INSTANCE,
) {
    // SAFETY: `ctx` is the slot passed with the request; it outlives the callback (`Drop` leaks
    // slots whose callback has not run).
    let slot = unsafe { &*ctx.cast::<ResolveSlot>() };
    if status == 0 && !instance.is_null() {
        // SAFETY: a valid instance from Windows, freed just below.
        if let Some((key, advert, ip)) = unsafe { parse_instance(&*instance) } {
            lock(&slot.shared.ids).insert(key, advert.id);
            slot.shared
                .send(DiscoveryEvent::Seen(advert, ip, Source::Mdns));
        }
    }
    if !instance.is_null() {
        // SAFETY: Windows hands the caller this instance to free.
        unsafe { DnsServiceFreeInstance(instance) };
    }
    slot.done.store(true, Ordering::Release);
}

/// # Safety
/// `inst` comes from a resolve callback: its strings and arrays are valid for `dwPropertyCount`.
unsafe fn parse_instance(inst: &DNS_SERVICE_INSTANCE) -> Option<(String, Advert, IpAddr)> {
    // SAFETY: per the function contract.
    let name = unsafe { wide(inst.pszInstanceName) }?;
    let mut pairs = Vec::new();
    if !inst.keys.is_null() && !inst.values.is_null() {
        for i in 0..inst.dwPropertyCount as usize {
            // SAFETY: both arrays hold `dwPropertyCount` entries.
            let pair = unsafe { (wide(*inst.keys.add(i)), wide(*inst.values.add(i))) };
            if let (Some(key), Some(value)) = pair {
                pairs.push((key, value));
            }
        }
    }
    let advert = Advert::from_txt(&pairs, inst.wPort)?;
    // SAFETY: non-null address pointers point to one address each; the union is plain bytes.
    let ip = unsafe {
        if !inst.ip4Address.is_null() {
            IpAddr::V4(Ipv4Addr::from((*inst.ip4Address).to_ne_bytes()))
        } else if !inst.ip6Address.is_null() {
            IpAddr::V6(Ipv6Addr::from((*inst.ip6Address).IP6Byte))
        } else {
            return None;
        }
    };
    Some((name.to_lowercase(), advert, ip))
}

impl MdnsBrowse {
    pub(crate) fn start(tx: Sender<DiscoveryEvent>) -> Result<MdnsBrowse, DiscoveryError> {
        let shared: &'static Shared = Box::leak(Box::new(Shared {
            closed: AtomicBool::new(false),
            tx: Mutex::new(Some(tx)),
            ids: Mutex::default(),
            found: Mutex::default(),
            lost: Mutex::default(),
        }));
        let query = HSTRING::from(SERVICE_TYPE);
        let request = DNS_SERVICE_BROWSE_REQUEST {
            Version: DNS_QUERY_REQUEST_VERSION1.0,
            InterfaceIndex: 0,
            QueryName: PCWSTR(query.as_ptr()),
            Anonymous: DNS_SERVICE_BROWSE_REQUEST_0 {
                pBrowseCallback: Some(on_browse),
            },
            pQueryContext: std::ptr::from_ref(shared).cast_mut().cast(),
        };
        let mut cancel = Box::new(DNS_SERVICE_CANCEL::default());
        // SAFETY: `request` and `query` live through the call; the context is `'static`.
        let code = unsafe { DnsServiceBrowse(&request, &mut *cancel) };
        if code != DNS_REQUEST_PENDING {
            shared.closed.store(true, Ordering::Release);
            return Err(win32("DnsServiceBrowse", code as u32));
        }
        Ok(MdnsBrowse {
            shared,
            cancel,
            known: HashMap::new(),
            slots: HashMap::new(),
        })
    }

    /// Starts resolves for new instances; with `refresh`, re-resolves every known one so their
    /// sightings stay younger than the host list expiry.
    pub(crate) fn poll(&mut self, refresh: bool) {
        self.slots
            .retain(|_, (slot, _)| !slot.done.load(Ordering::Acquire));
        for key in std::mem::take(&mut *lock(&self.shared.lost)) {
            self.known.remove(&key);
        }
        let found = std::mem::take(&mut *lock(&self.shared.found));
        for name in found {
            let key = name.to_lowercase();
            if self.known.insert(key.clone(), name.clone()).is_none() {
                self.resolve(key, &name);
            }
        }
        if refresh {
            let known: Vec<(String, String)> = self
                .known
                .iter()
                .map(|(key, name)| (key.clone(), name.clone()))
                .collect();
            for (key, name) in known {
                self.resolve(key, &name);
            }
        }
    }

    fn resolve(&mut self, key: String, name: &str) {
        if self.slots.contains_key(&key) {
            return;
        }
        let slot = Box::new(ResolveSlot {
            shared: self.shared,
            name: name.encode_utf16().chain(std::iter::once(0)).collect(),
            done: AtomicBool::new(false),
        });
        let mut cancel = Box::new(DNS_SERVICE_CANCEL::default());
        let request = DNS_SERVICE_RESOLVE_REQUEST {
            Version: DNS_QUERY_REQUEST_VERSION1.0,
            InterfaceIndex: 0,
            // Windows only reads the name.
            QueryName: PWSTR(slot.name.as_ptr().cast_mut()),
            pResolveCompletionCallback: Some(on_resolved),
            pQueryContext: std::ptr::from_ref::<ResolveSlot>(&slot).cast_mut().cast(),
        };
        // SAFETY: the name and context are boxed in `slot`, kept in `self.slots` (or leaked) until
        // the callback ran; `cancel` is a live box.
        let code = unsafe { DnsServiceResolve(&request, &mut *cancel) };
        if code == DNS_REQUEST_PENDING {
            self.slots.insert(key, (slot, cancel));
        } else {
            log::debug!("mDNS resolve of {name} refused ({code})");
        }
    }
}

impl Drop for MdnsBrowse {
    fn drop(&mut self) {
        self.shared.closed.store(true, Ordering::Release);
        *lock(&self.shared.tx) = None;
        // SAFETY: the handle filled by `DnsServiceBrowse`.
        unsafe { DnsServiceBrowseCancel(&*self.cancel) };
        for (_, (slot, cancel)) in self.slots.drain() {
            if !slot.done.load(Ordering::Acquire) {
                // SAFETY: the handle filled by `DnsServiceResolve` for this slot.
                unsafe { DnsServiceResolveCancel(&*cancel) };
                // The callback may still run: keep its context alive.
                Box::leak(slot);
            }
        }
    }
}
