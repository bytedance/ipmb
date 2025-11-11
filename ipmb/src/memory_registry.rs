use crate::MemoryRegion;
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

/// Perform memory region allocation by reusing system memory.
#[derive(Default)]
pub struct MemoryRegistry {
    inner: BTreeMap<usize, Vec<MemoryRegionEntry>>,
}

impl MemoryRegistry {
    /// Allocate a `MemoryRegion` from the `MemoryRegistry`, actual size may be larger than min_size.
    pub fn alloc(&mut self, min_size: usize, tag: Option<&str>) -> Option<MemoryRegion> {
        self.alloc_inner(min_size, tag, None)
    }

    pub fn alloc_with_free(
        &mut self,
        min_size: usize,
        tag: Option<&str>,
        free: impl FnOnce() + 'static,
    ) -> Option<MemoryRegion> {
        let free: Box<dyn FnOnce()> = Box::new(free);
        self.alloc_inner(min_size, tag, Some(free))
    }

    fn alloc_inner(
        &mut self,
        min_size: usize,
        tag: Option<&str>,
        free: Option<Box<dyn FnOnce()>>,
    ) -> Option<MemoryRegion> {
        let now = Instant::now();

        for (_, rs) in self.inner.range_mut(min_size..min_size * 2) {
            for r in rs {
                if r.region.ref_count_inner(0) == 1 && tag == r.tag.as_deref() {
                    r.last_alloc = now;

                    let region = r.region.clone().ok()?;
                    r.guard = Guard { free };

                    self.maintain_inner(now);

                    return Some(region);
                }
            }
        }

        self.maintain_inner(now);

        let r = MemoryRegion::new(min_size)?;
        self.inner
            .entry(min_size)
            .or_default()
            .push(MemoryRegionEntry {
                region: r.clone().ok()?,
                last_alloc: now,
                tag: tag.map(ToOwned::to_owned),
                guard: Guard { free },
            });
        Some(r)
    }

    fn maintain_inner(&mut self, now: Instant) {
        self.inner.retain(|_, rs| {
            rs.retain_mut(|r| {
                let should_retain = (now - r.last_alloc) < Duration::from_secs(5);

                if r.region.ref_count() == 1 {
                    r.guard = Guard { free: None };
                }

                should_retain
            });
            !rs.is_empty()
        });
    }

    pub fn maintain(&mut self) {
        self.maintain_inner(Instant::now());
    }
}

struct MemoryRegionEntry {
    region: MemoryRegion,
    last_alloc: Instant,
    tag: Option<String>,
    guard: Guard,
}

struct Guard {
    free: Option<Box<dyn FnOnce()>>,
}

impl Drop for Guard {
    fn drop(&mut self) {
        if let Some(free) = self.free.take() {
            free();
        }
    }
}
