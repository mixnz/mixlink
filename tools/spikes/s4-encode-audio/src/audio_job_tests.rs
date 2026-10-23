use super::*;

fn sum_sq(samples: &[f32]) -> f64 {
    samples.iter().map(|&v| f64::from(v) * f64::from(v)).sum()
}

#[test]
fn rms_of_full_scale_sine_is_minus_3() {
    let sine: Vec<f32> = (0..48_000)
        .map(|i| (2.0 * std::f64::consts::PI * 1_000.0 * f64::from(i) / 48_000.0).sin() as f32)
        .collect();
    let db = rms_dbfs(sum_sq(&sine), sine.len() as u64);
    assert!((db + 3.01).abs() < 0.05, "got {db}");
}

#[test]
fn rms_of_silence_is_floor() {
    assert_eq!(rms_dbfs(0.0, 48_000), -120.0);
    assert_eq!(rms_dbfs(1.0, 0), -120.0);
    assert_eq!(rms_dbfs(1e-20, 48_000), -120.0);
}

#[test]
fn loud_seconds_counts_threshold() {
    assert!(is_loud(-49.0));
    assert!(!is_loud(-51.0));
    assert!(!is_loud(-50.0));
}

#[test]
fn signed_percentiles_handle_negative_offsets() {
    let mut v: Vec<i64> = (-50..50).collect();
    assert_eq!(signed_percentiles(&mut v), (-50, -1, 48));
    assert_eq!(signed_percentiles(&mut []), (0, 0, 0));
}
