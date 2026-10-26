use super::*;

#[test]
fn instance_ids_carry_ig_00() {
    let id = pad_instance_id(0);
    assert!(id.starts_with(r"ROOT\"), "{id}");
    assert!(id.contains("VID_045E&PID_0B13&IG_00"), "{id}");
}

#[test]
fn instance_ids_differ_per_slot() {
    let ids: Vec<String> = (0..PAD_SLOTS).map(pad_instance_id).collect();
    for (i, a) in ids.iter().enumerate() {
        for b in &ids[i + 1..] {
            assert_ne!(a, b);
        }
    }
    assert!(pad_instance_id(3).ends_with(r"\0003"));
}

#[test]
fn container_guid_not_null_and_per_slot() {
    assert_ne!(pad_container_guid(0), 0);
    assert_ne!(pad_container_guid(0), pad_container_guid(1));
    assert_eq!(pad_container_guid(2) & 0xFF, 2);
}

#[test]
fn slot_check() {
    assert!(valid_slot(0) && valid_slot(3));
    assert!(!valid_slot(4));
}
