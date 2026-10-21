use super::*;

fn rec(t: u64, counter: Option<u32>) -> FrameRecord {
    FrameRecord {
        t_qpc: t,
        counter,
        ok: true,
        generation: 1,
        latency_qpc: 20,
        map_qpc: 5,
    }
}

#[test]
fn push_keeps_records_in_order() {
    let mut log = FrameLog::new(4);
    log.push(rec(1, Some(7)));
    log.push(rec(2, None));
    assert_eq!(log.records().len(), 2);
    assert_eq!(log.records()[1].t_qpc, 2);
}

#[test]
fn push_beyond_capacity_counts_overflow() {
    let mut log = FrameLog::new(1);
    log.push(rec(1, Some(7)));
    log.push(rec(2, Some(8)));
    assert_eq!(log.records().len(), 1);
    assert_eq!(log.overflow(), 1);
}

#[test]
fn csv_line_converts_ticks_to_us() {
    // 10 MHz clock: 20 ticks = 2 us, 5 ticks = 0 us (truncated).
    assert_eq!(csv_line(&rec(123, Some(7)), 10_000_000), "123,7,1,1,2,0");
    assert_eq!(csv_line(&rec(5, None), 10_000_000), "5,,1,1,2,0");
}
