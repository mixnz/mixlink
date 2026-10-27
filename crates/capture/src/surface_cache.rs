//! Textures opened from the driver's per-surface NT handles, opened once per handle (spec 0019).

/// IddCx rotates a few surfaces per swapchain; 8 leaves room without growing.
pub const CAPACITY: usize = 8;

/// Fixed-size cache keyed by the driver's handle value; the oldest entry is evicted when full.
#[derive(Debug)]
pub struct SurfaceCache<T> {
    entries: Vec<(u64, T)>,
    next_evict: usize,
}

impl<T> SurfaceCache<T> {
    pub fn new() -> Self {
        Self {
            entries: Vec::with_capacity(CAPACITY),
            next_evict: 0,
        }
    }

    // HOT PATH (a hit; a miss opens once per surface per generation)
    /// The cached value for `handle`, or `open`'s value, now cached. A failed open caches nothing.
    pub fn get_or_open<E>(
        &mut self,
        handle: u64,
        open: impl FnOnce(u64) -> Result<T, E>,
    ) -> Result<&T, E> {
        if let Some(i) = self.entries.iter().position(|(h, _)| *h == handle) {
            return Ok(&self.entries[i].1);
        }
        let value = open(handle)?;
        let i = if self.entries.len() < CAPACITY {
            self.entries.push((handle, value));
            self.entries.len() - 1
        } else {
            let i = self.next_evict;
            self.entries[i] = (handle, value);
            self.next_evict = (i + 1) % CAPACITY;
            i
        };
        Ok(&self.entries[i].1)
    }

    /// On a generation change: the old handles belong to the previous swapchain.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.next_evict = 0;
    }
}

impl<T> Default for SurfaceCache<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "surface_cache_tests.rs"]
mod tests;
