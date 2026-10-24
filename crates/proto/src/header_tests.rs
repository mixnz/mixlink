use super::*;

fn header(channel: Channel, flags: Flags, frag_index: u16, frag_count: u16) -> PacketHeader {
    PacketHeader {
        channel,
        flags,
        seq: Seq::new(0xDEAD_BEEF),
        frame: FrameId::new(7),
        frag_index,
        frag_count,
    }
}

/// Header + payload as one datagram.
fn datagram(h: &PacketHeader, payload: &[u8]) -> Vec<u8> {
    let mut bytes = [0u8; HEADER_LEN];
    h.encode(&mut bytes);
    let mut out = bytes.to_vec();
    out.extend_from_slice(payload);
    out
}

/// Valid datagram with one header byte range overwritten.
fn patched(offset: usize, value: &[u8], payload_len: usize) -> Vec<u8> {
    let mut out = datagram(
        &header(Channel::Video, Flags::empty(), 0, 1),
        &vec![0; payload_len],
    );
    out[offset..offset + value.len()].copy_from_slice(value);
    out
}

#[test]
fn header_round_trips_every_channel_and_flag() {
    let payload = [0xABu8; MAX_PAYLOAD];
    let flag_sets = [
        Flags::empty(),
        Flags::KEYFRAME,
        Flags::PARITY,
        Flags::KEYFRAME | Flags::PARITY,
    ];
    for channel in Channel::ALL {
        for flags in flag_sets {
            let h = header(channel, flags, 2, 5);
            let bytes = datagram(&h, &payload);
            let (decoded, body) = PacketHeader::decode(&bytes).unwrap();
            assert_eq!(decoded, h);
            assert_eq!(body, &payload[..]);
        }
    }
}

#[test]
fn header_bytes_are_little_endian() {
    let mut h = header(Channel::Audio, Flags::KEYFRAME, 0, 1);
    h.seq = Seq::new(0x0403_0201);
    let mut bytes = [0u8; HEADER_LEN];
    h.encode(&mut bytes);
    assert_eq!(bytes[0], WIRE_VERSION);
    assert_eq!(bytes[1], Channel::Audio.id());
    assert_eq!(bytes[2..4], [1, 0]);
    assert_eq!(bytes[4..8], [1, 2, 3, 4]);
    assert_eq!(bytes[8..12], [7, 0, 0, 0]);
    assert_eq!(bytes[12..16], [0, 0, 1, 0]);
}

#[test]
fn short_buffer_is_rejected() {
    assert_eq!(
        PacketHeader::decode(&[1u8; 15]),
        Err(DecodeError::TooShort { len: 15 })
    );
}

#[test]
fn bad_version_is_rejected() {
    let bytes = patched(0, &[2], 0);
    assert_eq!(
        PacketHeader::decode(&bytes),
        Err(DecodeError::Version { found: 2 })
    );
}

#[test]
fn bad_channel_is_rejected() {
    for id in [0u8, 7] {
        let bytes = patched(1, &[id], 0);
        assert_eq!(
            PacketHeader::decode(&bytes),
            Err(DecodeError::Channel { id })
        );
    }
}

#[test]
fn unknown_flag_bit_is_rejected() {
    let bytes = patched(2, &4u16.to_le_bytes(), 0);
    assert_eq!(
        PacketHeader::decode(&bytes),
        Err(DecodeError::Flags { bits: 4 })
    );
}

#[test]
fn zero_frag_count_is_rejected() {
    let bytes = patched(14, &0u16.to_le_bytes(), 0);
    assert_eq!(PacketHeader::decode(&bytes), Err(DecodeError::FragCount));
}

#[test]
fn frag_index_out_of_range_is_rejected() {
    let bytes = datagram(&header(Channel::Video, Flags::empty(), 5, 5), &[]);
    assert_eq!(
        PacketHeader::decode(&bytes),
        Err(DecodeError::FragIndex { index: 5, count: 5 })
    );
}

#[test]
fn oversized_payload_is_rejected() {
    let bytes = datagram(&header(Channel::Video, Flags::empty(), 0, 1), &[0; 1201]);
    assert_eq!(
        PacketHeader::decode(&bytes),
        Err(DecodeError::PayloadTooLarge { len: 1201 })
    );
}

#[test]
fn short_non_last_fragment_is_rejected() {
    let bytes = datagram(&header(Channel::Video, Flags::empty(), 0, 2), &[0; 1199]);
    assert_eq!(
        PacketHeader::decode(&bytes),
        Err(DecodeError::ShortFragment {
            index: 0,
            len: 1199
        })
    );
}

#[test]
fn last_fragment_may_be_short() {
    let bytes = datagram(&header(Channel::Video, Flags::empty(), 1, 2), &[9]);
    let (h, body) = PacketHeader::decode(&bytes).unwrap();
    assert_eq!(h.frag_index, 1);
    assert_eq!(body, &[9]);
}

#[test]
fn channel_from_id_round_trips() {
    for channel in Channel::ALL {
        assert_eq!(Channel::from_id(channel.id()), Some(channel));
    }
    assert_eq!(Channel::from_id(0), None);
    assert_eq!(Channel::from_id(7), None);
}

#[test]
fn flags_from_bits_rejects_unknown() {
    assert_eq!(Flags::from_bits(3), Some(Flags::KEYFRAME | Flags::PARITY));
    assert_eq!(Flags::from_bits(0x8000), None);
    assert!((Flags::KEYFRAME | Flags::PARITY).contains(Flags::PARITY));
    assert!(!Flags::KEYFRAME.contains(Flags::PARITY));
}
