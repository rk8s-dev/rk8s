use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

pub(crate) struct UsageGuard {
    usage: Arc<AtomicU64>,
    bytes: u64,
}

impl UsageGuard {
    pub(crate) fn new(usage: Arc<AtomicU64>) -> Self {
        Self { usage, bytes: 0 }
    }

    /// Updates the tracked byte count. This method must be called with exclusive access
    /// (e.g., behind a Mutex) to ensure consistent accounting across all guards sharing
    /// the same usage counter.
    pub(crate) fn update_bytes(&mut self, bytes: u64) {
        if bytes > self.bytes {
            self.usage.fetch_add(bytes - self.bytes, Ordering::Relaxed);
        } else {
            sub_usage(&self.usage, self.bytes - bytes);
        }

        self.bytes = bytes;
    }
}

impl Drop for UsageGuard {
    fn drop(&mut self) {
        sub_usage(&self.usage, self.bytes);
    }
}

fn sub_usage(usage: &AtomicU64, delta: u64) {
    let _ = usage.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |r| {
        Some(r.saturating_sub(delta))
    });
}
