use super::*;

#[test]
fn ping_round_trips() {
    let ping = Probe::Ping {
        seq: u32::MAX,
        sent: 0x0102_0304_0506_0708,
    };
    let mut buf = [0u8; 32];
    let len = ping.encode(&mut buf);
    assert_eq!(len, 13);
    assert_eq!(buf[0], 1);
    assert_eq!(Probe::decode(&buf[..len]), Some(ping));
}

#[test]
fn ack_round_trips() {
    let ack = Probe::Ack { frame: 77 };
    let mut buf = [0u8; 32];
    let len = ack.encode(&mut buf);
    assert_eq!(len, 5);
    assert_eq!(buf[0], 2);
    assert_eq!(Probe::decode(&buf[..len]), Some(ack));
}

#[test]
fn bad_kind_or_short_is_none() {
    for bytes in [&[][..], &[3], &[1, 0], &[2, 0, 0]] {
        assert_eq!(Probe::decode(bytes), None, "{bytes:?}");
    }
}
