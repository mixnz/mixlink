#[cfg(windows)]
#[test]
fn local_nets_have_no_loopback() {
    let nets = crate::platform::local_ipv4_nets().unwrap();
    assert!(nets.iter().all(|net| !net.addr.is_loopback()));
}

/// Real mDNS through the Windows responder; run with `--ignored` (needs a network, spec 0017).
#[cfg(windows)]
#[test]
#[ignore = "uses the machine's mDNS responder"]
fn mdns_register_then_browse_sees_it() {
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    use mixlink_core::ids::DeviceId;

    use crate::discovery::advert::{Advert, CAPS_V1};
    use crate::discovery::browser::DiscoveryEvent;
    use crate::discovery::list::Source;
    use crate::platform::{MdnsBrowse, MdnsRegistration};

    let mut id = [0u8; 16];
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    id.copy_from_slice(&nanos.to_le_bytes());
    let advert = Advert {
        id: DeviceId::from_bytes(id),
        name: format!("MixLink test {}", std::process::id()),
        ver: 1,
        caps: CAPS_V1.to_owned(),
        port: 47801,
    };
    let (tx, rx) = mpsc::channel();
    let mut browse = MdnsBrowse::start(tx).unwrap();
    let registration = MdnsRegistration::register(&advert).unwrap();

    let wait_for = |rx: &mpsc::Receiver<DiscoveryEvent>,
                    browse: &mut MdnsBrowse,
                    want: &dyn Fn(&DiscoveryEvent) -> bool| {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            browse.poll(false);
            if let Ok(event) = rx.recv_timeout(Duration::from_millis(100))
                && want(&event)
            {
                return true;
            }
        }
        false
    };
    let seen = |e: &DiscoveryEvent| matches!(e, DiscoveryEvent::Seen(a, _, Source::Mdns) if a.id == advert.id && a.name == advert.name);
    assert!(wait_for(&rx, &mut browse, &seen), "registration not seen");
    drop(registration);
    let gone = |e: &DiscoveryEvent| matches!(e, DiscoveryEvent::Gone(id, Source::Mdns) if *id == advert.id);
    assert!(wait_for(&rx, &mut browse, &gone), "goodbye not seen");
}
