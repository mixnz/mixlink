use super::*;

#[test]
fn utc_stamp_epoch() {
    assert_eq!(utc_stamp(0), "19700101-000000");
}

#[test]
fn utc_stamp_dates() {
    assert_eq!(utc_stamp(1_761_091_200), "20251022-000000");
    assert_eq!(utc_stamp(1_709_164_800 + 3_723), "20240229-010203");
}

#[test]
fn arg_value_reads_flag() {
    let args: Vec<String> = ["x", "--minutes", "10", "--case", "A"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    assert_eq!(arg_value(&args, "--case"), Some("A"));
    assert_eq!(arg_value(&args, "--minutes"), Some("10"));
    assert_eq!(arg_value(&args, "--mode"), None);
}
