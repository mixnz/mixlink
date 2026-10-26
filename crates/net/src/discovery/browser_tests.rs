#[cfg(windows)]
#[test]
fn local_nets_have_no_loopback() {
    let nets = crate::platform::local_ipv4_nets().unwrap();
    assert!(nets.iter().all(|net| !net.addr.is_loopback()));
}
