use super::*;

/// Every datagram of one message, copied out.
fn all_datagrams(p: &mut Packetizer, frame: u32, flags: Flags, message: &[u8]) -> Vec<Vec<u8>> {
    let mut fragments = p.packetize(FrameId::new(frame), flags, message).unwrap();
    let mut out = Vec::new();
    let mut buf = [0u8; MAX_DATAGRAM];
    while let Some(len) = fragments.next_into(&mut buf) {
        out.push(buf[..len].to_vec());
    }
    out
}

#[test]
fn fragment_count_values() {
    assert_eq!(fragment_count(0), Some(1));
    assert_eq!(fragment_count(1), Some(1));
    assert_eq!(fragment_count(1200), Some(1));
    assert_eq!(fragment_count(1201), Some(2));
    assert_eq!(fragment_count(1200 * 65535), Some(65535));
    assert_eq!(fragment_count(1200 * 65535 + 1), None);
}

#[test]
fn empty_message_is_one_empty_datagram() {
    let mut p = Packetizer::new(Channel::Control);
    let datagrams = all_datagrams(&mut p, 3, Flags::empty(), &[]);
    assert_eq!(datagrams.len(), 1);
    assert_eq!(datagrams[0].len(), HEADER_LEN);
    let (h, body) = PacketHeader::decode(&datagrams[0]).unwrap();
    assert_eq!((h.frag_index, h.frag_count), (0, 1));
    assert!(body.is_empty());
}

#[test]
fn datagrams_carry_message_in_order() {
    let message: Vec<u8> = (0..2500u32).map(|i| i as u8).collect();
    let mut p = Packetizer::new(Channel::Video);
    let datagrams = all_datagrams(&mut p, 9, Flags::KEYFRAME, &message);
    let lens: Vec<usize> = datagrams.iter().map(Vec::len).collect();
    assert_eq!(lens, [1216, 1216, 116]);
    let mut joined = Vec::new();
    for (i, d) in datagrams.iter().enumerate() {
        let (h, body) = PacketHeader::decode(d).unwrap();
        assert_eq!(h.channel, Channel::Video);
        assert_eq!(h.frame, FrameId::new(9));
        assert_eq!(h.flags, Flags::KEYFRAME);
        assert_eq!(h.frag_index, i as u16);
        assert_eq!(h.frag_count, 3);
        joined.extend_from_slice(body);
    }
    assert_eq!(joined, message);
}

#[test]
fn seq_increments_per_datagram_and_wraps() {
    let mut p = Packetizer::with_seq(Channel::Video, Seq::new(u32::MAX - 1));
    let datagrams = all_datagrams(&mut p, 0, Flags::empty(), &[0; 2401]);
    let seqs: Vec<u32> = datagrams
        .iter()
        .map(|d| PacketHeader::decode(d).unwrap().0.seq.get())
        .collect();
    assert_eq!(seqs, [u32::MAX - 1, u32::MAX, 0]);
    assert_eq!(p.next_seq(), Seq::new(1));
}

#[test]
fn fragments_report_count() {
    let mut p = Packetizer::new(Channel::Video);
    let fragments = p
        .packetize(FrameId::new(0), Flags::empty(), &[0; 2401])
        .unwrap();
    assert_eq!(fragments.count(), 3);
}

#[test]
fn too_large_message_is_rejected() {
    let message = vec![0u8; 1200 * 65535 + 1];
    let mut p = Packetizer::new(Channel::Video);
    let result = p.packetize(FrameId::new(0), Flags::empty(), &message);
    assert_eq!(
        result.err(),
        Some(PacketizeError::TooLarge { len: message.len() })
    );
    assert_eq!(p.next_seq(), Seq::new(0));
}
