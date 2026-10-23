use super::*;

#[test]
fn not_slower_relative_and_floor() {
    assert!(not_slower(3300, 3000));
    assert!(!not_slower(3301, 3000));
    assert!(not_slower(1200, 1000));
    assert!(!not_slower(1201, 1000));
}

#[test]
fn parse_samples_skips_blank_and_comments() {
    let samples = parse_samples_csv("# encode us\n10\n\n 20 \n30\n").unwrap();
    assert_eq!(samples, vec![10, 20, 30]);
    assert!(parse_samples_csv("10\nx\n").is_err());
}

#[test]
fn pool_reports_both_sides() {
    let a: Vec<u64> = (1..=100).collect();
    let b: Vec<u64> = (1..=100).map(|v| v * 2).collect();
    let text = pool(&[('A', vec![a.clone(), a]), ('B', vec![b])]);
    assert!(
        text.contains("side A: rounds=2 samples=200 p50=50 p99=99 max=100 us; round p99 [99, 99]")
    );
    assert!(text.contains("side B: rounds=1 samples=100 p50=100 p99=198 max=200 us"));
    assert!(text.contains("rule A vs B: p99 99 <= max(217, 398) us -> PASS"));
}

#[test]
fn pool_without_side_b_says_so() {
    let text = pool(&[('C', vec![vec![5]])]);
    assert!(text.contains("rule: no side B"));
}
