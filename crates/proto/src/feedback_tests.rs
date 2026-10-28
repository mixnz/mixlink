use mixlink_core::ids::FrameId;

use super::*;

fn round_trip(msg: Feedback) -> Feedback {
    let mut out = [0u8; FEEDBACK_LEN];
    let len = msg.encode(&mut out);
    assert_eq!(len, FEEDBACK_LEN);
    Feedback::decode(&out[..len]).unwrap()
}

#[test]
fn keyframe_request_round_trips() {
    let msg = Feedback::KeyframeRequest {
        last_good: FrameId::new(0xDEAD_BEEF),
    };
    assert_eq!(round_trip(msg), msg);
}

#[test]
fn cursor_shape_request_round_trips() {
    let msg = Feedback::CursorShapeRequest { shape_id: 77 };
    assert_eq!(round_trip(msg), msg);
}

#[test]
fn unknown_kind_is_error() {
    assert_eq!(
        Feedback::decode(&[9, 0, 0, 0, 0]),
        Err(FeedbackError::Kind { byte: 9 })
    );
}

#[test]
fn short_is_error() {
    assert_eq!(
        Feedback::decode(&[1, 0, 0]),
        Err(FeedbackError::TooShort { len: 3 })
    );
    assert_eq!(
        Feedback::decode(&[]),
        Err(FeedbackError::TooShort { len: 0 })
    );
}
