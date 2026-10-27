use mixlink_core::ids::{FrameId, Seq};

use super::*;
use crate::header::{Flags, HEADER_LEN, PacketHeader};

const BUF: usize = 64;

fn keys() -> SessionKeys {
    SessionKeys {
        c2h: [1u8; KEY_LEN],
        h2c: [2u8; KEY_LEN],
    }
}

/// A datagram on `channel` with `seq` and an 8-byte payload, in a buffer with room for the tag.
fn datagram(channel: Channel, seq: u32) -> ([u8; BUF], usize) {
    let mut buf = [0u8; BUF];
    let header = PacketHeader {
        channel,
        flags: Flags::empty(),
        seq: Seq::new(seq),
        frame: FrameId::new(7),
        frag_index: 0,
        frag_count: 1,
    };
    let mut head = [0u8; HEADER_LEN];
    header.encode(&mut head);
    buf[..HEADER_LEN].copy_from_slice(&head);
    buf[HEADER_LEN..HEADER_LEN + 8].copy_from_slice(b"payload!");
    (buf, HEADER_LEN + 8)
}

fn sealed(sealer: &mut Sealer, seq: u32) -> Vec<u8> {
    let (mut buf, len) = datagram(Channel::Input, seq);
    let total = sealer.seal(&mut buf, len).unwrap();
    buf[..total].to_vec()
}

fn pair() -> (Sealer, Opener) {
    let k = keys();
    (
        k.sealer(Role::Client, Channel::Input),
        k.opener(Role::Host, Channel::Input),
    )
}

#[test]
fn seal_open_round_trip() {
    let (mut sealer, mut opener) = pair();
    let (plain, len) = datagram(Channel::Input, 5);
    let mut buf = sealed(&mut sealer, 5);
    assert_eq!(buf.len(), len + TAG_LEN);
    assert_eq!(buf[..HEADER_LEN], plain[..HEADER_LEN]);
    assert_ne!(buf[HEADER_LEN..len], plain[HEADER_LEN..len]);
    assert_eq!(opener.open(&mut buf), Ok(len));
    assert_eq!(buf[..len], plain[..len]);
    assert_eq!(opener.stats().opened, 1);
}

fn tamper_at(index: impl Fn(usize) -> usize) {
    let (mut sealer, mut opener) = pair();
    let mut buf = sealed(&mut sealer, 1);
    let at = index(buf.len());
    buf[at] ^= 0x01;
    assert_eq!(opener.open(&mut buf), Err(CryptoError::BadTag));
    assert_eq!(opener.stats().bad_tag, 1);
}

#[test]
fn tampered_header_fails() {
    tamper_at(|_| 12);
}

#[test]
fn tampered_payload_fails() {
    tamper_at(|_| HEADER_LEN + 2);
}

#[test]
fn tampered_tag_fails() {
    tamper_at(|len| len - 1);
}

#[test]
fn wrong_direction_fails() {
    let k = keys();
    let mut sealer = k.sealer(Role::Client, Channel::Input);
    let mut opener = k.opener(Role::Client, Channel::Input);
    let mut buf = sealed(&mut sealer, 1);
    assert_eq!(opener.open(&mut buf), Err(CryptoError::BadTag));
}

#[test]
fn replay_dropped() {
    let (mut sealer, mut opener) = pair();
    let buf = sealed(&mut sealer, 9);
    assert!(opener.open(&mut buf.clone()).is_ok());
    assert_eq!(opener.open(&mut buf.clone()), Err(CryptoError::Replayed));
    assert_eq!(opener.stats().replayed, 1);
}

#[test]
fn reordered_within_window_opens() {
    let (mut sealer, mut opener) = pair();
    let packets: Vec<Vec<u8>> = (1..=5).map(|s| sealed(&mut sealer, s)).collect();
    for i in [4, 0, 2, 1, 3] {
        assert!(
            opener.open(&mut packets[i].clone()).is_ok(),
            "seq {}",
            i + 1
        );
    }
}

#[test]
fn older_than_window_dropped() {
    let (mut sealer, mut opener) = pair();
    let old = sealed(&mut sealer, 1);
    let new = sealed(&mut sealer, 1 + WINDOW as u32);
    assert!(opener.open(&mut new.clone()).is_ok());
    assert_eq!(opener.open(&mut old.clone()), Err(CryptoError::TooOld));
    assert_eq!(opener.stats().too_old, 1);
}

#[test]
fn just_inside_window_opens() {
    let (mut sealer, mut opener) = pair();
    let old = sealed(&mut sealer, 2);
    let new = sealed(&mut sealer, 1 + WINDOW as u32);
    assert!(opener.open(&mut new.clone()).is_ok());
    assert!(opener.open(&mut old.clone()).is_ok());
}

#[test]
fn window_tracks_across_words() {
    let (mut sealer, mut opener) = pair();
    let seqs = [10u32, 75, 140, 700, 1000];
    let packets: Vec<Vec<u8>> = seqs.iter().map(|&s| sealed(&mut sealer, s)).collect();
    for i in [4, 1, 3, 0, 2] {
        assert!(
            opener.open(&mut packets[i].clone()).is_ok(),
            "seq {}",
            seqs[i]
        );
    }
    for packet in &packets {
        assert_eq!(opener.open(&mut packet.clone()), Err(CryptoError::Replayed));
    }
}

#[test]
fn sealer_refuses_same_seq() {
    let (mut sealer, _) = pair();
    sealed(&mut sealer, 3);
    let (mut buf, len) = datagram(Channel::Input, 3);
    assert_eq!(sealer.seal(&mut buf, len), Err(CryptoError::SeqNotNewer));
    let (mut buf, len) = datagram(Channel::Input, 2);
    assert_eq!(sealer.seal(&mut buf, len), Err(CryptoError::SeqNotNewer));
}

#[test]
fn open_follows_seq_wrap() {
    let (mut sealer, mut opener) = pair();
    for seq in (u32::MAX - 2..=u32::MAX).chain(0..=2) {
        let mut buf = sealed(&mut sealer, seq);
        assert!(opener.open(&mut buf).is_ok(), "seq {seq}");
    }
}

#[test]
fn open_reordered_across_wrap() {
    let (mut sealer, mut opener) = pair();
    let first = sealed(&mut sealer, u32::MAX - 1);
    let before = sealed(&mut sealer, u32::MAX);
    let after = sealed(&mut sealer, 1);
    assert!(opener.open(&mut first.clone()).is_ok());
    assert!(opener.open(&mut after.clone()).is_ok());
    assert!(opener.open(&mut before.clone()).is_ok());
    assert_eq!(opener.open(&mut before.clone()), Err(CryptoError::Replayed));
}

#[test]
fn wrong_channel_refused() {
    let (mut sealer, mut opener) = pair();
    let (mut buf, len) = datagram(Channel::Video, 1);
    assert_eq!(sealer.seal(&mut buf, len), Err(CryptoError::WrongChannel));
    let mut video = keys().sealer(Role::Client, Channel::Video);
    let total = video.seal(&mut buf, len).unwrap();
    assert_eq!(
        opener.open(&mut buf[..total]),
        Err(CryptoError::WrongChannel)
    );
}

#[test]
fn no_room_for_tag_refused() {
    let (mut sealer, _) = pair();
    let (mut buf, len) = datagram(Channel::Input, 1);
    assert_eq!(
        sealer.seal(&mut buf[..len + 4], len),
        Err(CryptoError::NoRoom)
    );
}

#[test]
fn too_short_refused() {
    let (_, mut opener) = pair();
    let mut buf = [0u8; HEADER_LEN + TAG_LEN - 1];
    assert_eq!(opener.open(&mut buf), Err(CryptoError::TooShort));
}

#[test]
fn debug_hides_keys() {
    let k = SessionKeys {
        c2h: [0xab; KEY_LEN],
        h2c: [0xcd; KEY_LEN],
    };
    let text = format!(
        "{k:?} {:?} {:?}",
        k.sealer(Role::Host, Channel::Control),
        k.opener(Role::Host, Channel::Control)
    );
    assert!(!text.contains("ab") && !text.contains("171"), "{text}");
}
