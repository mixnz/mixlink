use super::*;

const HEADER: &str = "t_qpc,counter,ok,generation,latency_us,map_us";

fn csv(rows: &[(&str, u64)]) -> String {
    let mut t = String::from(HEADER);
    for (i, (counter, latency)) in rows.iter().enumerate() {
        t.push_str(&format!("\n{},{counter},1,1,{latency},5", 1_000 + i));
    }
    t
}

fn round(name: &str, rows: &[(&str, u64)], levers: &str) -> RoundInput {
    RoundInput {
        name: name.to_string(),
        csv: csv(rows),
        cond: vec![
            format!("cond levers {levers}"),
            "cond nvidia-smi P2, 2100 MHz".to_string(),
        ],
    }
}

#[test]
fn parse_frames_csv_counts_lost() {
    let (lat, s) = parse_frames_csv(&csv(&[("1", 100), ("2", 200), ("5", 300), ("", 0)])).unwrap();
    assert_eq!(lat, vec![100, 200, 300]);
    assert_eq!(s.delivered, 3);
    assert_eq!(s.lost, 2);
    assert_eq!(s.undecodable, 1);
}

#[test]
fn parse_frames_csv_rejects_bad_row() {
    assert!(parse_frames_csv(&format!("{HEADER}\n1,x,1,1,5,5")).is_err());
}

#[test]
fn pool_merges_rounds() {
    let a = round("r1", &[("1", 100), ("2", 100), ("3", 900)], "3");
    let mut b = round("r2", &[("1", 100), ("3", 2_000)], "3");
    b.cond[1] = "cond nvidia-smi P0, 2600 MHz".to_string(); // sampled per round: ignored
    let text = pool(&[a, b]).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[0], "cond levers 3");
    assert_eq!(
        lines[1],
        "pooled 2 rounds: p50 100 us, p99 2000 us, max 2000 us (5 samples); lost 1 / 6 (16.667 %), undecodable 0"
    );
    assert_eq!(
        lines[2],
        "round r1: p99 900 us, lost 0 / 3 (0.000 %), undecodable 0"
    );
    assert_eq!(
        lines[3],
        "round r2: p99 2000 us, lost 1 / 3 (33.333 %), undecodable 0"
    );
}

#[test]
fn pool_rejects_mixed_conditions() {
    let a = round("r1", &[("1", 100)], "3");
    let b = round("r2", &[("1", 100)], "1");
    assert!(pool(&[a, b]).is_err());
}

#[test]
fn pool_rejects_empty() {
    assert!(pool(&[]).is_err());
}

#[test]
fn pool_flags_covered_round() {
    // 3 delivered, 2 duplicates (> 1 %): the counter cell stopped changing.
    let a = round("r1", &[("1", 100), ("2", 100), ("3", 100)], "1");
    let b = round(
        "r2",
        &[("1", 100), ("1", 0), ("1", 0), ("2", 100), ("3", 100)],
        "1",
    );
    let text = pool(&[a, b]).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert!(!lines[2].contains("INVALID"), "{}", lines[2]);
    assert_eq!(
        lines[3],
        "round r2: p99 100 us, lost 0 / 3 (0.000 %), undecodable 0, duplicates 2 INVALID: pattern covered or stopped"
    );
    assert_eq!(
        lines[4],
        "invalid rounds 1 of 2: re-run them with nothing over the pattern"
    );
}
