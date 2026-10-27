use super::*;

#[test]
fn generated_keys_differ() {
    let a = StaticKeyPair::generate().unwrap();
    let b = StaticKeyPair::generate().unwrap();
    assert_ne!(a.private, b.private);
    assert_ne!(a.public, b.public);
}

#[test]
fn pin_below_million() {
    for _ in 0..10_000 {
        assert!(random_pin().unwrap() < 1_000_000);
    }
}

#[test]
fn key_pair_debug_hides_private() {
    let pair = StaticKeyPair {
        private: [0xee; 32],
        public: PublicKey([0x11; 32]),
    };
    let text = format!("{pair:?}");
    assert!(!text.contains("ee") && !text.contains("238"), "{text}");
}

#[test]
fn key_pair_json_round_trips() {
    let pair = StaticKeyPair::generate().unwrap();
    let json = serde_json::to_string(&pair).unwrap();
    assert_eq!(serde_json::from_str::<StaticKeyPair>(&json).unwrap(), pair);
}
