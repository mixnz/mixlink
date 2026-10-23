use super::*;

#[test]
fn match_by_pts_skips_older() {
    let mut pending = vec![Some(10), Some(20), None];
    assert_eq!(record(&mut pending, 1, 25), Some(5));
    assert_eq!(pending, vec![Some(10), None, None]);
}

#[test]
fn record_ignores_unknown_pts() {
    let mut pending = vec![Some(10), None];
    assert_eq!(record(&mut pending, 1, 30), None);
    assert_eq!(record(&mut pending, -1, 30), None);
    assert_eq!(record(&mut pending, 99, 30), None);
    assert_eq!(record(&mut pending, 0, 30), Some(20));
    assert_eq!(record(&mut pending, 0, 40), None);
}

#[test]
fn stats_counts_lost() {
    assert_eq!(count_lost(&[None, Some(1), None, Some(2)]), 2);
    assert_eq!(count_lost(&[]), 0);
}
