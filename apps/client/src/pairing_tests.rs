use std::net::Ipv4Addr;

use mixlink_net::discovery::{CAPS_V1, HostEntry};
use mixlink_net::pairing::KnownHost;
use mixlink_proto::control::PublicKey;
use mixlink_proto::header::WIRE_VERSION;

use super::*;
use crate::hosts::rows;

#[test]
fn pin_input_rules() {
    assert_eq!(parse_pin(" 012345 "), Some(12_345));
    assert_eq!(parse_pin("999999"), Some(999_999));
    assert_eq!(parse_pin("12345"), None);
    assert_eq!(parse_pin("1234567"), None);
    assert_eq!(parse_pin("12a456"), None);
    assert_eq!(parse_pin("12 456"), None);
    assert_eq!(parse_pin("+12345"), None);
    assert_eq!(parse_pin("١٢٣٤٥٦"), None);
}

#[test]
fn failed_reason_maps() {
    assert_eq!(fail_reason(&PairError::Closed), FailReason::Closed);
    assert_eq!(fail_reason(&PairError::Busy), FailReason::Busy);
    assert_eq!(fail_reason(&PairError::Version), FailReason::Version);
    assert_eq!(
        fail_reason(&PairError::Net(NetError::Timeout)),
        FailReason::NoAnswer
    );
    assert_eq!(
        fail_reason(&PairError::Net(NetError::UnexpectedReply)),
        FailReason::Failed
    );
}

#[test]
fn state_serializes_camel_case() {
    let state = PairState::AwaitingPin {
        host: "ab".to_owned(),
        expires_s: 90,
        wrong_pin: true,
    };
    assert_eq!(
        serde_json::to_string(&state).unwrap(),
        r#"{"kind":"awaitingPin","host":"ab","expiresS":90,"wrongPin":true}"#
    );
    let failed = PairState::Failed {
        host: "ab".to_owned(),
        reason: FailReason::NoAnswer,
    };
    assert_eq!(
        serde_json::to_string(&failed).unwrap(),
        r#"{"kind":"failed","host":"ab","reason":"noAnswer"}"#
    );
}

#[test]
fn forget_marks_row_not_paired() {
    let dir = std::env::temp_dir().join(format!("mixlink-launcher-forget-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let host = DeviceId::from_bytes([5; 16]);
    client_store::remember(
        &dir,
        KnownHost {
            host,
            name: "Desk".to_owned(),
            key: PublicKey([5; 32]),
        },
    )
    .unwrap();
    let entry = HostEntry {
        id: host,
        name: "Desk".to_owned(),
        ver: WIRE_VERSION,
        caps: CAPS_V1.to_owned(),
        port: 47801,
        address: IpAddr::V4(Ipv4Addr::new(192, 168, 1, 5)),
    };
    assert!(rows(std::slice::from_ref(&entry), &known_ids_in(&dir))[0].paired);
    client_store::forget(&dir, host).unwrap();
    assert!(!rows(&[entry], &known_ids_in(&dir))[0].paired);
}
