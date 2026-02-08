use dashmap::DashMap;
use tracing::info;

/// Local ID allocation pool
///
/// This structure maintains each pool of a specific counter_key.
#[derive(Default)]
pub struct IdPool {
    inner: DashMap<String, IdPoolInner>,
}

impl IdPool {
    /// Try allocating an id from the pool of `counter_key`.
    pub fn try_alloc(&self, counter_key: impl AsRef<str>) -> Option<i64> {
        let counter_key = counter_key.as_ref();

        // Notice that the entry api will lock current thread, but it's ok there because we will not
        // hold it for a long time or cross the await boundary.
        let mut entry = self.inner.entry(counter_key.to_string()).or_default();

        let pool = entry.value_mut();
        if pool.next < pool.end {
            let id = pool.next;
            pool.next += 1;
            let remaining = pool.end - pool.next;

            info!(
                counter_key = counter_key,
                allocated_id = id,
                pool_hit = true,
                pool_remaining = remaining,
                "ID allocated from pool (fast path)"
            );

            return Some(id);
        }
        None
    }

    /// Update the new `next` and `end` of the pool of `counter_key`
    pub fn update(&self, counter_key: impl AsRef<str>, next: i64, end: i64) {
        let counter_key = counter_key.as_ref();

        // if everything is ok, there must be an occupied entry.
        let mut entry = self.inner.entry(counter_key.to_string()).or_default();

        let pool = entry.value_mut();
        pool.next = next;
        pool.end = end;
    }
}

/// Local ID allocation pool Inner
///
/// This structure maintains a range of pre-allocated IDs from etcd.
/// Must be protected by a Mutex for thread-safe access, as multiple
/// async tasks may attempt to allocate IDs concurrently.
///
/// The pool allocates BATCH_SIZE IDs from etcd at once and distributes
/// them locally to reduce network round-trips.
#[derive(Default)]
struct IdPoolInner {
    /// Next ID to allocate from local pool
    next: i64,
    /// End of current pool range (exclusive)
    end: i64,
}
