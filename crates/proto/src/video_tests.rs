use super::*;

fn meta() -> FrameMeta {
    FrameMeta {
        keyframe: true,
        capture_us: 410,
        encode_us: 2780,
        send_us: 55,
    }
}

#[test]
fn meta_round_trip() {
    let mut msg = vec![0u8; FRAME_META_LEN];
    let head = msg.first_chunk_mut::<FRAME_META_LEN>().unwrap();
    meta().encode(head);
    msg.extend_from_slice(&[0, 0, 0, 1, 0x65, 0xAA]);
    let (decoded, au) = FrameMeta::split(&msg).unwrap();
    assert_eq!(decoded, meta());
    assert_eq!(au, &[0, 0, 0, 1, 0x65, 0xAA]);
}

#[test]
fn meta_not_keyframe_round_trip() {
    let mut head = [0u8; FRAME_META_LEN];
    let mut sent = meta();
    sent.keyframe = false;
    sent.encode(&mut head);
    let (decoded, au) = FrameMeta::split(&head).unwrap();
    assert_eq!(decoded, sent);
    assert!(au.is_empty());
}

#[test]
fn meta_short_message_is_error() {
    let msg = [1u8; FRAME_META_LEN - 1];
    assert_eq!(
        FrameMeta::split(&msg),
        Err(VideoError::TooShort {
            len: FRAME_META_LEN - 1
        })
    );
}

#[test]
fn meta_unknown_version_is_error() {
    let mut head = [0u8; FRAME_META_LEN];
    meta().encode(&mut head);
    head[0] = 2;
    assert_eq!(
        FrameMeta::split(&head),
        Err(VideoError::Version { found: 2 })
    );
}

#[test]
fn meta_unknown_flags_are_error() {
    let mut head = [0u8; FRAME_META_LEN];
    meta().encode(&mut head);
    head[1] = 0b10;
    assert_eq!(
        FrameMeta::split(&head),
        Err(VideoError::Flags { bits: 0b10 })
    );
}
