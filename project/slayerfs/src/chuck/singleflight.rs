//! SingleFlight: coalesce concurrent requests for the same key into a single operation.
//!
//! When multiple concurrent requests arrive for the same key, only the first one
//! actually executes the underlying operation. All subsequent requests wait for
//! the first to complete and share its result.
//!
//! A single flight only covers callers that join while the key is still present in
//! the in-flight map. Once the leader finishes, the key is removed from the map
//! before the result is published to existing waiters. This means callers that were
//! already attached to the current flight share the same result, while later callers
//! start a new flight and execute their own closure.
//!
//! This is particularly useful for:
//! - Avoiding thundering herd effects on cache misses
//! - Reducing redundant object storage requests
//! - Saving network bandwidth and compute resources
//!
//! # Example
//!
//! ```ignore
//! use slayerfs::chuck::singleflight::SingleFlight;
//!
//! let sf: SingleFlight<String, Vec<u8>> = SingleFlight::new();
//!
//! // Multiple concurrent calls with the same key will only execute once
//! let result = sf.execute("my-key".to_string(), || async {
//!     fetch_from_object_storage("my-key").await
//! }).await;
//! ```

use std::future::Future;
use std::hash::Hash;
use std::sync::Arc;

use dashmap::{DashMap, Entry};
use parking_lot::Mutex;
use tokio::sync::Notify;

pub type SharedError = Arc<anyhow::Error>;

type SharedResult<V> = Result<Arc<V>, SharedError>;

enum EntryState<V> {
    Running,
    Ready(SharedResult<V>),
}

struct FlightEntry<V> {
    state: Mutex<EntryState<V>>,
    notify: Notify,
}

impl<V> FlightEntry<V> {
    fn new() -> Self {
        Self {
            state: Mutex::new(EntryState::Running),
            notify: Notify::new(),
        }
    }

    fn result(&self) -> Option<SharedResult<V>> {
        let state = self.state.lock();

        match &*state {
            EntryState::Running => None,
            EntryState::Ready(result) => Some(result.clone()),
        }
    }

    fn finish(&self, result: SharedResult<V>) {
        let mut state = self.state.lock();

        if let EntryState::Running = &*state {
            *state = EntryState::Ready(result);

            drop(state);
            self.notify.notify_waiters();
        }
    }
}

struct LeaderGuard<'a, K, V>
where
    K: Hash + Eq + Clone,
{
    parent: &'a SingleFlight<K, V>,
    key: K,
    entry: Arc<FlightEntry<V>>,
    completed: bool,
}

impl<'a, K, V> LeaderGuard<'a, K, V>
where
    K: Hash + Eq + Clone,
{
    fn new(parent: &'a SingleFlight<K, V>, key: K, entry: Arc<FlightEntry<V>>) -> Self {
        Self {
            parent,
            key,
            entry,
            completed: false,
        }
    }

    fn complete(&mut self, result: SharedResult<V>) -> SharedResult<V> {
        self.parent.remove_entry(&self.key, &self.entry);
        self.entry.finish(result.clone());
        self.completed = true;
        result
    }
}

impl<K, V> Drop for LeaderGuard<'_, K, V>
where
    K: Hash + Eq + Clone,
{
    fn drop(&mut self) {
        if !self.completed {
            let err = Arc::new(anyhow::anyhow!(
                "SingleFlight executor dropped before completing",
            ));

            self.parent.remove_entry(&self.key, &self.entry);
            self.entry.finish(Err(err));
        }
    }
}

/// SingleFlight controller that coalesces concurrent requests for the same key.
///
/// Thread-safe: internally synchronized with `DashMap` and `parking_lot::Mutex`, and safe
/// to share across tasks/threads. Intended to live as long as the store/client instance so
/// all calls with
/// the same key can be coalesced.
///
/// Type parameters:
/// - `K`: The key type (must be `Hash + Eq + Clone`)
/// - `V`: The value type. The result is shared as `Arc<V>` to avoid copying.
pub struct SingleFlight<K, V> {
    /// Map of keys to in-flight requests
    in_flight: DashMap<K, Arc<FlightEntry<V>>>,
}

impl<K, V> Default for SingleFlight<K, V>
where
    K: Hash + Eq + Clone,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<K, V> SingleFlight<K, V>
where
    K: Hash + Eq + Clone,
{
    /// Create a new SingleFlight controller.
    pub fn new() -> Self {
        Self {
            in_flight: DashMap::new(),
        }
    }

    /// Execute an async operation, coalescing concurrent requests for the same key.
    ///
    /// If there's already an in-flight request for this key, wait for its result.
    /// Otherwise, execute the provided function and share its result with any
    /// concurrent waiters.
    ///
    /// Flight semantics:
    /// - callers that observe the key in `in_flight` join the current flight
    /// - the leader removes the key from `in_flight` before publishing the result
    /// - existing waiters still receive that published result through their shared entry
    /// - callers arriving after removal start a new flight instead of replaying the old result
    ///
    /// # Arguments
    ///
    /// * `key` - The key identifying this request
    /// * `f` - An async function to execute if this is the first request for this key
    ///
    /// # Returns
    ///
    /// Returns `Ok(Arc<V>)` with the shared result on success. The result is wrapped
    /// in `Arc` to enable zero-copy sharing across all concurrent waiters.
    /// Returns `Err(Arc<anyhow::Error>)` if the operation fails.
    ///
    /// # Performance Note
    ///
    /// Uses `DashMap` for per-key in-flight coordination and `parking_lot::Mutex` for
    /// per-entry state transitions, keeping lock scope small without async-aware mutex overhead.
    pub async fn execute<F, Fut, E>(&self, key: K, f: F) -> SharedResult<V>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<V, E>>,
        E: Into<anyhow::Error>,
    {
        let (entry, is_leader) = match self.in_flight.entry(key.clone()) {
            Entry::Occupied(entry) => (entry.get().clone(), false),
            Entry::Vacant(entry) => {
                let flight = Arc::new(FlightEntry::new());
                entry.insert(flight.clone());
                (flight, true)
            }
        };

        if !is_leader {
            return Self::wait_for_result(&entry).await;
        }

        let mut leader = LeaderGuard::new(self, key, entry.clone());

        let shared_result = match f().await {
            Ok(v) => Ok(Arc::new(v)),
            Err(e) => Err(Arc::new(e.into())),
        };

        leader.complete(shared_result)
    }

    async fn wait_for_result(entry: &Arc<FlightEntry<V>>) -> SharedResult<V> {
        loop {
            let notified = entry.notify.notified();

            if let Some(result) = entry.result() {
                return result;
            }

            notified.await;
        }
    }

    fn remove_entry(&self, key: &K, entry: &Arc<FlightEntry<V>>) {
        let should_remove = self
            .in_flight
            .get(key)
            .is_some_and(|current| Arc::ptr_eq(current.value(), entry));

        if should_remove {
            self.in_flight.remove(key);
        }
    }

    /// Check the number of currently in-flight requests.
    ///
    /// Useful for metrics and debugging.
    #[allow(dead_code)]
    pub fn in_flight_count(&self) -> usize {
        self.in_flight.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    #[tokio::test]
    async fn test_single_request() {
        let sf: SingleFlight<String, String> = SingleFlight::new();
        let counter = Arc::new(AtomicUsize::new(0));

        let counter_clone = counter.clone();
        let result = sf
            .execute("key1".to_string(), || async move {
                counter_clone.fetch_add(1, Ordering::SeqCst);
                Ok::<_, std::io::Error>("value1".to_string())
            })
            .await
            .unwrap();

        assert_eq!(*result, "value1");
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_concurrent_requests_coalesced() {
        let sf = Arc::new(SingleFlight::<String, String>::new());
        let counter = Arc::new(AtomicUsize::new(0));

        let mut handles = Vec::new();

        // Spawn 10 concurrent requests for the same key
        for _ in 0..10 {
            let sf_clone = sf.clone();
            let counter_clone = counter.clone();
            handles.push(tokio::spawn(async move {
                sf_clone
                    .execute("same-key".to_string(), || async move {
                        // Simulate some work
                        tokio::time::sleep(Duration::from_millis(50)).await;
                        counter_clone.fetch_add(1, Ordering::SeqCst);
                        Ok::<_, std::io::Error>("shared-value".to_string())
                    })
                    .await
            }));
        }

        // Wait for all requests to complete
        let results: Vec<_> = futures::future::join_all(handles)
            .await
            .into_iter()
            .map(|r| r.unwrap())
            .collect();

        // All should get the same value
        for result in &results {
            assert_eq!(**result.as_ref().unwrap(), "shared-value");
        }

        // But the operation should only have executed once
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_different_keys_not_coalesced() {
        let sf = Arc::new(SingleFlight::<String, String>::new());
        let counter = Arc::new(AtomicUsize::new(0));

        let mut handles = Vec::new();

        // Spawn requests for different keys
        for i in 0..5 {
            let sf_clone = sf.clone();
            let counter_clone = counter.clone();
            let key = format!("key-{}", i);
            handles.push(tokio::spawn(async move {
                sf_clone
                    .execute(key.clone(), || async move {
                        counter_clone.fetch_add(1, Ordering::SeqCst);
                        Ok::<_, std::io::Error>(format!("value-{}", key))
                    })
                    .await
            }));
        }

        futures::future::join_all(handles).await;

        // Each key should execute separately
        assert_eq!(counter.load(Ordering::SeqCst), 5);
    }

    #[tokio::test]
    async fn test_error_propagation() {
        let sf: SingleFlight<String, String> = SingleFlight::new();

        let result = sf
            .execute("error-key".to_string(), || async move {
                Err::<String, _>(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "test error",
                ))
            })
            .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("test error"));
    }

    #[tokio::test]
    async fn test_concurrent_requests_executor_fails() {
        let sf = Arc::new(SingleFlight::<String, String>::new());

        let mut handles = Vec::new();
        for _ in 0..5 {
            let sf_clone = sf.clone();
            handles.push(tokio::spawn(async move {
                sf_clone
                    .execute("key".to_string(), || async {
                        tokio::time::sleep(Duration::from_millis(50)).await;
                        Err::<String, _>(std::io::Error::new(
                            std::io::ErrorKind::NotFound,
                            "intentional failure",
                        ))
                    })
                    .await
            }));
        }

        let results: Vec<_> = futures::future::join_all(handles)
            .await
            .into_iter()
            .map(|r| r.unwrap())
            .collect();

        for result in results {
            assert!(result.is_err());
            assert!(
                result
                    .unwrap_err()
                    .to_string()
                    .contains("intentional failure")
            );
        }

        assert_eq!(
            sf.in_flight_count(),
            0,
            "in-flight map should be cleaned up"
        );
    }

    #[tokio::test]
    async fn test_late_caller_starts_new_flight() {
        let sf = Arc::new(SingleFlight::<String, String>::new());
        let counter = Arc::new(AtomicUsize::new(0));
        let started = Arc::new(Notify::new());
        let release = Arc::new(Notify::new());

        let sf_clone = sf.clone();
        let counter_clone = counter.clone();
        let started_clone = started.clone();
        let release_clone = release.clone();

        let leader = tokio::spawn(async move {
            sf_clone
                .execute("key".to_string(), || async move {
                    started_clone.notify_one();
                    release_clone.notified().await;

                    counter_clone.fetch_add(1, Ordering::SeqCst);
                    Ok::<_, std::io::Error>("first".to_string())
                })
                .await
        });

        started.notified().await;

        let waiter_sf = sf.clone();
        let waiter = tokio::spawn(async move {
            waiter_sf
                .execute("key".to_string(), || async move {
                    panic!("late waiter should join the existing flight");
                    #[allow(unreachable_code)]
                    Ok::<_, std::io::Error>("unexpected".to_string())
                })
                .await
        });

        release.notify_one();

        let leader_result = leader.await.unwrap().unwrap();
        let waiter_result = waiter.await.unwrap().unwrap();

        assert_eq!(&*leader_result, "first");
        assert_eq!(&*waiter_result, "first");
        assert_eq!(counter.load(Ordering::SeqCst), 1);

        let counter_clone = counter.clone();
        let next = sf
            .execute("key".to_string(), || async move {
                counter_clone.fetch_add(1, Ordering::SeqCst);
                Ok::<_, std::io::Error>("second".to_string())
            })
            .await
            .unwrap();

        assert_eq!(&*next, "second");
        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn test_sequential_requests_both_execute() {
        let sf: SingleFlight<String, String> = SingleFlight::new();
        let counter = Arc::new(AtomicUsize::new(0));

        // First request
        {
            let counter_clone = counter.clone();
            sf.execute("key".to_string(), || async move {
                counter_clone.fetch_add(1, Ordering::SeqCst);
                Ok::<_, std::io::Error>("first".to_string())
            })
            .await
            .unwrap();
        }

        // Second request (after first completes)
        {
            let counter_clone = counter.clone();
            sf.execute("key".to_string(), || async move {
                counter_clone.fetch_add(1, Ordering::SeqCst);
                Ok::<_, std::io::Error>("second".to_string())
            })
            .await
            .unwrap();
        }

        // Both should execute since they're sequential
        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn test_executor_cancellation_cleans_up_and_allows_retry() {
        let sf = Arc::new(SingleFlight::<String, String>::new());
        let started = Arc::new(Notify::new());

        let sf_clone = sf.clone();
        let started_clone = started.clone();

        let leader = tokio::spawn(async move {
            sf_clone
                .execute("key".to_string(), || async move {
                    started_clone.notify_one();
                    futures::future::pending::<()>().await;
                    #[allow(unreachable_code)]
                    Ok::<_, std::io::Error>("never".to_string())
                })
                .await
        });

        started.notified().await;
        leader.abort();
        tokio::task::yield_now().await;

        assert_eq!(sf.in_flight_count(), 0);

        let retry = sf
            .execute("key".to_string(), || async move {
                Ok::<_, std::io::Error>("retry".to_string())
            })
            .await
            .unwrap();

        assert_eq!(&*retry, "retry");
    }
}
