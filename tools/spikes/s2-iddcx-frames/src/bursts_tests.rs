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

fn log(counters: &[Option<u32>]) -> Vec<FrameRecord> {
    counters
        .iter()
        .enumerate()
        .map(|(i, c)| rec(1_000 + i as u64 * 10, *c))
        .collect()
}

#[test]
fn bursts_from_counter_gaps() {
    let records = log(&[Some(1), Some(2), Some(5), Some(6), None, None, Some(9)]);
    let b = bursts(&records);
    assert_eq!(
        b,
        vec![
            Burst {
                start_qpc: 1_020,
                lost: 2,
                undecodable: 0
            },
            Burst {
                start_qpc: 1_040,
                lost: 2,
                undecodable: 2
            },
        ]
    );
}

#[test]
fn bursts_empty_when_clean() {
    let records = log(&[Some(1), Some(2), Some(2), Some(3)]);
    assert!(bursts(&records).is_empty());
}

#[test]
fn burst_open_at_end_is_kept() {
    let records = log(&[Some(1), None]);
    assert_eq!(
        bursts(&records),
        vec![Burst {
            start_qpc: 1_010,
            lost: 0,
            undecodable: 1
        }]
    );
}

#[test]
fn burst_notes_caps_at_20() {
    let b: Vec<Burst> = (0..25)
        .map(|i| Burst {
            start_qpc: 1_000 + i * 1_000,
            lost: i as u32 + 1,
            undecodable: 0,
        })
        .collect();
    let notes = burst_notes(&b, 1_000, 1_000);
    assert_eq!(notes.len(), 21);
    assert_eq!(notes[0], "loss bursts 25, lost 325, undecodable 0");
    assert_eq!(notes[1], "burst at 24000 ms: lost 25, undecodable 0");
}
