//! SingleFlight: coalesce concurrent requests for the same key into a single operation.
//!
//! When multiple concurrent requests arrive for the same key, only the first one
//! actually executes the underlying operation. All subsequent requests wait for
//! the first to complete and share its result.
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

use std::collections::HashMap;
use std::future::Future;
use std::hash::Hash;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;

pub type SharedError = Arc<anyhow::Error>;

/// A request in flight, tracking the broadcast channel for result sharing.
struct InFlight<V> {
    /// Sender for broadcasting the result to all waiters
    tx: broadcast::Sender<Result<Arc<V>, SharedError>>,
}

/// SingleFlight controller that coalesces concurrent requests for the same key.
///
/// Type parameters:
/// - `K`: The key type (must be `Hash + Eq + Clone`)
/// - `V`: The value type. The result is shared as `Arc<V>` to avoid copying.
pub struct SingleFlight<K, V> {
    /// Map of keys to in-flight requests
    in_flight: Mutex<HashMap<K, InFlight<V>>>,
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
            in_flight: Mutex::new(HashMap::new()),
        }
    }

    /// Execute an async operation, coalescing concurrent requests for the same key.
    ///
    /// If there's already an in-flight request for this key, wait for its result.
    /// Otherwise, execute the provided function and share its result with any
    /// concurrent waiters.
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
    /// Returns `Err(Arc<String>)` if the operation fails.
    ///
    /// # Performance Note
    ///
    /// Uses `std::sync::Mutex` for the in-flight map since lock hold time is minimal
    /// (only HashMap operations). This provides better performance than `tokio::sync::Mutex`
    /// for this use case (10-20ns vs 100-200ns per lock operation).
    pub async fn execute<F, Fut, E>(&self, key: K, f: F) -> Result<Arc<V>, SharedError>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<V, E>>,
        E: Into<anyhow::Error>,
    {
        // Check if there's already an in-flight request
        let mut rx = {
            let mut guard = self.in_flight.lock().unwrap();
            if let Some(in_flight) = guard.get(&key) {
                // Subscribe to the existing request's result
                Some(in_flight.tx.subscribe())
            } else {
                // Create a new in-flight entry
                // Use a channel capacity of 1 since we only send one result
                let (tx, _) = broadcast::channel(1);
                guard.insert(key.clone(), InFlight { tx });
                None
            }
        };

        // If we're a waiter, wait for the result
        if let Some(ref mut rx) = rx {
            return match rx.recv().await {
                Ok(result) => result,
                Err(_) => Err(Arc::new(anyhow::anyhow!("SingleFlight: channel closed"))),
            };
        }

        // We're the executor - run the actual operation.
        // IMPORTANT: do not clone `V` here. For large values (e.g. `Vec<u8>`) cloning would
        // duplicate the entire buffer. Instead, wrap the result once in `Arc` and share it.
        let shared_result: Result<Arc<V>, SharedError> = match f().await {
            Ok(v) => Ok(Arc::new(v)),
            Err(e) => Err(Arc::new(e.into())),
        };

        // Remove from in-flight and broadcast result
        {
            let mut guard = self.in_flight.lock().unwrap();
            if let Some(in_flight) = guard.remove(&key) {
                // Ignore send errors - no receivers means no one is waiting
                let _ = in_flight.tx.send(shared_result.clone());
            }
        }

        shared_result
    }

    /// Check the number of currently in-flight requests.
    ///
    /// Useful for metrics and debugging.
    #[allow(dead_code)]
    pub fn in_flight_count(&self) -> usize {
        self.in_flight.lock().unwrap().len()
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
}
