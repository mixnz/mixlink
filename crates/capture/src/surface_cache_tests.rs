use std::cell::Cell;

use super::*;

fn open_counting(calls: &Cell<u32>) -> impl FnOnce(u64) -> Result<u64, ()> + '_ {
    move |handle| {
        calls.set(calls.get() + 1);
        Ok(handle * 10)
    }
}

#[test]
fn opens_once_per_handle() {
    let calls = Cell::new(0);
    let mut cache = SurfaceCache::new();
    assert_eq!(cache.get_or_open(7, open_counting(&calls)), Ok(&70));
    assert_eq!(cache.get_or_open(7, open_counting(&calls)), Ok(&70));
    assert_eq!(calls.get(), 1);
}

#[test]
fn failed_open_caches_nothing() {
    let mut cache: SurfaceCache<u64> = SurfaceCache::new();
    assert_eq!(cache.get_or_open(1, |_| Err("refused")), Err("refused"));
    assert_eq!(cache.get_or_open(1, Ok::<_, &str>), Ok(&1));
}

#[test]
fn evicts_oldest_past_capacity() {
    let calls = Cell::new(0);
    let mut cache = SurfaceCache::new();
    for handle in 0..=CAPACITY as u64 {
        let _ = cache.get_or_open(handle, open_counting(&calls));
    }
    assert_eq!(calls.get(), CAPACITY as u32 + 1);
    // Handle 0 was the oldest: opened again; the newest stays cached.
    let _ = cache.get_or_open(0, open_counting(&calls));
    let _ = cache.get_or_open(CAPACITY as u64, open_counting(&calls));
    assert_eq!(calls.get(), CAPACITY as u32 + 2);
}

#[test]
fn generation_change_clears_cache() {
    let calls = Cell::new(0);
    let mut cache = SurfaceCache::new();
    let _ = cache.get_or_open(3, open_counting(&calls));
    cache.clear();
    let _ = cache.get_or_open(3, open_counting(&calls));
    assert_eq!(calls.get(), 2);
}
