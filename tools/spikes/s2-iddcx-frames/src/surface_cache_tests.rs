use super::*;

fn opener(calls: &mut u32) -> impl FnOnce(u64) -> BoxResult<u64> + '_ {
    move |h| {
        *calls += 1;
        Ok(h * 10)
    }
}

#[test]
fn surface_cache_opens_once() {
    let mut cache = SurfaceCache::new();
    let mut calls = 0;
    assert_eq!(cache.get_or_open(7, opener(&mut calls)), Some(&70));
    assert_eq!(cache.get_or_open(7, opener(&mut calls)), Some(&70));
    assert_eq!(calls, 1);
}

#[test]
fn surface_cache_rejects_unknown_handle() {
    let mut cache: SurfaceCache<u64> = SurfaceCache::new();
    assert_eq!(cache.get_or_open(5, |_| Err("refused".into())), None);
    let mut calls = 0;
    assert_eq!(cache.get_or_open(5, opener(&mut calls)), Some(&50));
    assert_eq!(calls, 1, "a failed open must not be cached");
}

#[test]
fn surface_cache_full_evicts_oldest() {
    let mut cache = SurfaceCache::new();
    let mut calls = 0;
    for h in 1..=9 {
        cache.get_or_open(h, opener(&mut calls));
    }
    assert_eq!(calls, 9);
    cache.get_or_open(2, opener(&mut calls));
    assert_eq!(calls, 9, "handle 2 is still cached");
    cache.get_or_open(1, opener(&mut calls));
    assert_eq!(calls, 10, "handle 1 was evicted by the 9th");
}

#[test]
fn surface_cache_clear_forgets_all() {
    let mut cache = SurfaceCache::new();
    let mut calls = 0;
    cache.get_or_open(3, opener(&mut calls));
    cache.clear();
    cache.get_or_open(3, opener(&mut calls));
    assert_eq!(calls, 2);
}
