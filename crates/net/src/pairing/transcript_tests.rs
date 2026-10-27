use super::*;

fn id(byte: u8) -> DeviceId {
    DeviceId::from_bytes([byte; 16])
}

fn parts<'a>(
    ids: &'a (DeviceId, DeviceId),
    keys: &'a (PublicKey, PublicKey),
    spake: &'a [u8],
) -> Parts<'a> {
    Parts {
        device: &ids.0,
        host: &ids.1,
        client_key: &keys.0,
        host_key: &keys.1,
        spake_a: spake,
        spake_b: spake,
    }
}

#[test]
fn swapping_a_key_changes_transcript() {
    let ids = (id(1), id(2));
    let keys = (PublicKey([3; 32]), PublicKey([4; 32]));
    let swapped = (PublicKey([4; 32]), PublicKey([3; 32]));
    let spake = [5u8; 33];
    assert_ne!(
        transcript(&parts(&ids, &keys, &spake)),
        transcript(&parts(&ids, &swapped, &spake))
    );
}

#[test]
fn confirm_keys_differ() {
    let (h, c) = confirm_keys(b"shared", &[7; 32]);
    assert_ne!(h, c);
}

#[test]
fn mac_verifies_and_rejects_changes() {
    let t = [9u8; 32];
    let key = [1u8; 32];
    let tag = mac(&key, &t);
    assert!(verify(&key, &t, &tag));
    let mut bad = tag;
    bad[0] ^= 1;
    assert!(!verify(&key, &t, &bad));
    assert!(!verify(&[2; 32], &t, &tag));
    assert!(!verify(&key, &t, &tag[..31]));
}
