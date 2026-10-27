use super::*;

fn prompt(pin: &str, expires_s: u32) -> PromptState {
    PromptState {
        device_name: "Laptop".to_owned(),
        pin: pin.to_owned(),
        expires_s,
    }
}

#[test]
fn new_prompt_shows_window() {
    assert!(should_show(None, Some(&prompt("123456", 120))));
    assert!(should_show(
        Some(&prompt("123456", 30)),
        Some(&prompt("654321", 120))
    ));
}

#[test]
fn same_prompt_does_not_reshow() {
    assert!(!should_show(
        Some(&prompt("123456", 120)),
        Some(&prompt("123456", 119))
    ));
    assert!(!should_show(Some(&prompt("123456", 1)), None));
}

#[test]
fn rows_carry_no_key_and_hex_id() {
    let rows = rows(vec![PairedInfo {
        device: DeviceId::from_bytes([0xab; 16]),
        name: "Laptop".to_owned(),
        paired_at: 7,
    }]);
    assert_eq!(rows[0].device, "ab".repeat(16));
    let json = serde_json::to_string(&rows).unwrap();
    assert_eq!(
        json,
        format!(
            r#"[{{"device":"{}","name":"Laptop","pairedAt":7}}]"#,
            "ab".repeat(16)
        )
    );
}

#[test]
fn bad_device_id_is_refused_before_the_pipe() {
    assert!(matches!(remove("nope"), Err(TrayPairError::Device(_))));
}
