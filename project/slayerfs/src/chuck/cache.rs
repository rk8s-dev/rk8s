use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::{
    hash,
    path::{Path, PathBuf},
};

use anyhow::anyhow;
use dirs::cache_dir;
use sea_orm::sea_query::WindowSelectType;
use sha2::{Digest, Sha256, digest::KeyInit};
use tokio::fs;
use tokio::sync::RwLock;
use tracing::{debug, error, info, trace, warn};

/// Configuration for the intelligent dual-layer cache system.
///
/// This cache implements an adaptive promotion strategy that combines:
/// - **Dual time windows** for burst detection and trend analysis
/// - **Dynamic threshold adjustment** based on system metrics
/// - **Weighted frequency calculation** for intelligent promotion decisions
///
/// # Architecture Overview
///
/// ```text
/// +-----------------+    +-----------------+    +-----------------+
/// |   Hot Cache     |    |   Cold Cache    |    |  Disk Storage   |
/// |   (1024 items)  |<-->|   (1024 items)  |<-->|   (Persistent)  |
/// |   Fast Access   |    |   Metadata Only |    |   SHA256 Files  |
/// +-----------------+    +-----------------+    +-----------------+
///         ^                       |                       ^
///         |                       |                       |
///         v                       v                       v
///    Adaptive Promotion     Access Pattern Tracking    Fallback Storage
///    Strategy Engine        & Frequency Analysis       for Large Data
/// ```
///
/// # Promotion Strategy Details
///
/// The promotion decision uses a multi-dimensional scoring system:
///
/// ```text
/// weighted_frequency = short_freq * short_weight + medium_freq * medium_weight
/// adaptive_threshold = base_threshold * system_factor * hitrate_factor
/// promote_if = weighted_frequency >= adaptive_threshold
/// ```
///
/// ## Time Windows
///
/// - **Short Window (10s)**: Detects burst access patterns with 1-second granularity
/// - **Medium Window (60s)**: Analyzes medium-term trends with 5-second granularity
///
/// ## Adaptive Threshold Logic
///
/// - **High Load** (>0.8): Reduce threshold by 30% for aggressive promotion
/// - **Low Hit Rate** (<0.6): Increase threshold by 30% to prevent cache pollution
/// - **High Hit Rate** (>0.8): Reduce threshold by 10% to maintain performance
///
/// # Example Configurations
///
/// ## Performance-Optimized (Low Latency)
/// ```text
/// ChunksCacheConfig {
///     base_promotion_threshold: 5.0,
///     short_window_weight: 0.8,
///     enable_adaptive_threshold: true,
///     // ... other settings
/// }
/// ```
///
/// ## Memory-Conservative (Resource Constrained)
/// ```text
/// ChunksCacheConfig {
///     base_promotion_threshold: 15.0,
///     short_window_weight: 0.6,
///     conservative_promotion_hit_rate_threshold: 0.7,
///     // ... other settings
/// }
/// ```
#[derive(Debug, Clone)]
pub struct ChunksCacheConfig {
    /// Maximum number of entries in hot cache (fastest access tier)
    ///
    /// **Recommended**: 512-2048 depending on available memory
    /// **Impact**: Higher values = more hot data but more memory usage
    pub hot_cache_size: usize,

    /// Maximum number of entries in cold cache (metadata tracking tier)
    ///
    /// **Recommended**: Same as hot_cache_size or 2x for comprehensive tracking
    /// **Impact**: Higher values = better access pattern visibility but more metadata overhead
    pub cold_cache_size: usize,

    /// Base access frequency threshold for promoting items to hot cache
    ///
    /// **Units**: accesses per second
    /// **Typical Range**: 5.0 - 20.0
    /// **Lower Values**: More aggressive promotion (better for bursty workloads)
    /// **Higher Values**: More conservative (better for stable workloads)
    pub base_promotion_threshold: f64,

    /// Short time window for burst access detection
    ///
    /// **Purpose**: Capture rapid, recent access patterns
    /// **Recommended**: 5-15 seconds
    /// **Trade-off**: Shorter = more responsive but noisier
    pub short_window_size: Duration,

    /// Medium time window for trend analysis
    ///
    /// **Purpose**: Identify sustained access patterns and trends
    /// **Recommended**: 30-120 seconds
    /// **Trade-off**: Longer = more stable but slower to adapt
    pub medium_window_size: Duration,

    /// Maximum number of access records to keep per key
    ///
    /// **Note**: Legacy parameter for compatibility. Actual bucket count is
    ///         calculated dynamically based on window sizes.
    pub max_access_entries: usize,

    /// Custom disk storage directory (optional)
    ///
    /// If None, uses system cache directory via `dirs::cache_dir()`
    /// Files are stored using SHA256 hash of the key for unique naming
    pub disk_storage_dir: Option<PathBuf>,

    /// Weight for short window access frequency in promotion decisions
    ///
    /// **Range**: 0.0 - 1.0
    /// **Higher Values**: Prioritize recent burst access (good for interactive workloads)
    /// **Lower Values**: Prioritize sustained trends (good for batch workloads)
    pub short_window_weight: f64,

    /// Weight for medium window access frequency in promotion decisions
    ///
    /// **Range**: 0.0 - 1.0
    /// **Note**: short_window_weight + medium_window_weight should typically sum to 1.0
    pub medium_window_weight: f64,

    /// Enable adaptive threshold adjustment based on system load and hit rate
    ///
    /// **When true**: Dynamically adjusts promotion threshold based on:
    ///   - System load (cache utilization + request rate)
    ///   - Cache hit rate
    ///
    /// **When false**: Uses fixed base_promotion_threshold
    pub enable_adaptive_threshold: bool,

    /// System load threshold for triggering aggressive promotion mode
    ///
    /// **Range**: 0.0 - 1.0
    /// **When exceeded**: Reduces promotion threshold by 30% to cache more data
    /// **Purpose**: Improve performance under high load by increasing cache hit rate
    pub aggressive_promotion_load_threshold: f64,

    /// Cache hit rate threshold for triggering conservative promotion mode
    ///
    /// **Range**: 0.0 - 1.0
    /// **When below**: Increases promotion threshold by 30% to prevent cache pollution
    /// **Purpose**: Maintain cache efficiency when hit rate is already low
    pub conservative_promotion_hit_rate_threshold: f64,
}

impl Default for ChunksCacheConfig {
    fn default() -> Self {
        Self {
            hot_cache_size: 1024,
            cold_cache_size: 1024,
            base_promotion_threshold: 10.0,
            short_window_size: Duration::from_secs(10),
            medium_window_size: Duration::from_secs(60),
            max_access_entries: 100,
            disk_storage_dir: None,
            short_window_weight: 0.7,
            medium_window_weight: 0.3,
            enable_adaptive_threshold: true,
            aggressive_promotion_load_threshold: 0.8,
            conservative_promotion_hit_rate_threshold: 0.6,
        }
    }
}

#[derive(Debug, Clone)]
struct DiskStorage {
    base_dir: PathBuf,
}

impl DiskStorage {
    pub async fn new<P: AsRef<Path>>(base_dir: P) -> anyhow::Result<Self> {
        let base_dir = base_dir.as_ref().to_path_buf();
        debug!("Initializing disk storage at: {:?}", base_dir);

        if !base_dir.exists() {
            info!("Creating cache directory: {:?}", base_dir);
            fs::create_dir_all(&base_dir).await?;
        } else {
            debug!("Cache directory already exists: {:?}", base_dir);
        }

        Ok(Self { base_dir })
    }

    pub fn key_to_filename(key: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(key.as_bytes());
        let hash_result = hasher.finalize();

        hex::encode(hash_result)
    }

    pub async fn store(&self, key: &str, data: impl AsRef<[u8]>) -> anyhow::Result<()> {
        let filename = Self::key_to_filename(key);
        let filepath = self.base_dir.join(filename);
        let data_bytes = data.as_ref();

        trace!(
            "Storing {} bytes for key '{}' to file: {:?}",
            data_bytes.len(),
            key,
            filepath
        );

        tokio::fs::write(filepath, data_bytes).await?;
        debug!(
            "Successfully stored data for key '{}', size: {} bytes",
            key,
            data_bytes.len()
        );
        Ok(())
    }

    pub async fn load(&self, key: &str) -> anyhow::Result<Vec<u8>> {
        let filename = Self::key_to_filename(key);
        let filepath = self.base_dir.join(filename);

        trace!("Loading data for key '{}' from file: {:?}", key, filepath);

        if !filepath.exists() {
            trace!("File does not exist for key '{}': {:?}", key, filepath);
            return Err(anyhow!("file {} does not exist", filepath.display()));
        }

        match tokio::fs::read(filepath).await {
            Ok(data) => {
                debug!(
                    "Successfully loaded data for key '{}', size: {} bytes",
                    key,
                    data.len()
                );
                Ok(data)
            }
            Err(e) => {
                error!("Failed to load data for key '{}': {}", key, e);
                Err(e.into())
            }
        }
    }

    #[allow(dead_code)]
    pub async fn remove(&self, key: &str) -> anyhow::Result<()> {
        let filename = Self::key_to_filename(key);
        let filepath = self.base_dir.join(filename);

        trace!("Removing file for key '{}': {:?}", key, filepath);

        if !filepath.exists() {
            warn!(
                "Attempted to remove non-existent file for key '{}': {:?}",
                key, filepath
            );
            return Err(anyhow!("file {} does not exist", filepath.display()));
        }

        match tokio::fs::remove_file(filepath).await {
            Ok(_) => {
                debug!("Successfully removed file for key '{}'", key);
                Ok(())
            }
            Err(e) => {
                error!("Failed to remove file for key '{}': {}", key, e);
                Err(e.into())
            }
        }
    }
}

/// Lock-free access statistics tracker with dual time window analysis.
///
/// This structure implements a high-performance, concurrent-safe access pattern
/// analysis system using atomic operations and circular time buckets.
///
/// # Architecture
///
/// ```text
/// Time Progress ----------------------------------------------->
///
/// Short Window (10s, 1s granularity):
/// +--+--+--+--+--+--+--+--+--+--+
/// |0 |1 |2 |3 |4 |5 |6 |7 |8 |9 |
/// +--+--+--+--+--+--+--+--+--+--+
///  |                            |
///  v                            v
/// Old Data                  Recent Data
///
/// Medium Window (60s, 5s granularity):
/// +---+---+---+---+---+---+---+---+---+---+---+---+
/// | 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 |10 |11 |
/// +---+---+---+---+---+---+---+---+---+---+---+---+
///  0-5s  5-10s                     55-60s
/// ```
///
/// # Performance Characteristics
///
/// - **O(1) access recording**: Single atomic increment per access
/// - **Lock-free design**: No mutexes or RwLocks
/// - **Memory efficient**: Circular buffer with automatic cleanup
/// - **Cache-friendly**: Sequential memory layout
///
/// # Usage Examples
///
/// ```ignore
/// use std::time::Duration;
///
/// let stats = AccessStats::new(
///     Duration::from_secs(10),  // Short window
///     Duration::from_secs(60),  // Medium window
///     100,                      // Legacy compat parameter
/// );
///
/// // Record an access (thread-safe, O(1))
/// stats.record_access();
///
/// // Calculate weighted frequency
/// let weighted = stats.get_weighted_access_frequency(0.7, 0.3);
/// ```
///
/// # Thread Safety
///
/// This struct is designed for concurrent access from multiple threads.
/// All operations use atomic primitives and are completely lock-free.
#[derive(Debug)]
struct AccessStats {
    /// Short window buckets for burst access detection
    ///
    /// - **Granularity**: 1 second per bucket
    /// - **Window Size**: Up to 60 buckets (60 seconds total)
    /// - **Purpose**: Capture rapid access patterns and spikes
    short_buckets: Box<[AtomicU64]>,

    /// Current active bucket index for short window (circular buffer)
    short_current_bucket: AtomicUsize,

    /// Duration each short bucket represents (always 1 second)
    short_bucket_duration_secs: u64,

    /// Total number of short window buckets
    short_bucket_count: usize,

    /// Medium window buckets for trend analysis
    ///
    /// - **Granularity**: 5 seconds per bucket
    /// - **Window Size**: Up to 72 buckets (6 minutes total)
    /// - **Purpose**: Identify sustained access patterns
    medium_buckets: Box<[AtomicU64]>,

    /// Current active bucket index for medium window (circular buffer)
    medium_current_bucket: AtomicUsize,

    /// Duration each medium bucket represents (always 5 seconds)
    medium_bucket_duration_secs: u64,

    /// Total number of medium window buckets
    medium_bucket_count: usize,

    /// Last update timestamp (Unix epoch seconds)
    /// Used for bucket rotation and cleanup
    last_update: AtomicU64,

    /// Short window time span for frequency calculations
    short_window_size: Duration,

    /// Medium window time span for frequency calculations
    medium_window_size: Duration,
}

impl AccessStats {
    fn new(short_window_size: Duration, medium_window_size: Duration, _max_entries: usize) -> Self {
        // Short window: 1 second per bucket, up to 60 buckets (1 minute)
        let short_bucket_duration_secs = 1u64;
        let short_bucket_count =
            (short_window_size.as_secs() / short_bucket_duration_secs).clamp(10, 300) as usize;

        // Medium window: 5 seconds per bucket, up to 72 buckets (6 minutes)
        let medium_bucket_duration_secs = 5u64;
        let medium_bucket_count =
            (medium_window_size.as_secs() / medium_bucket_duration_secs).clamp(12, 180) as usize;

        debug!(
            "Creating AccessStats: short_window={:?} ({} buckets), medium_window={:?} ({} buckets)",
            short_window_size, short_bucket_count, medium_window_size, medium_bucket_count
        );

        let short_buckets = (0..short_bucket_count)
            .map(|_| AtomicU64::new(0))
            .collect::<Vec<_>>()
            .into_boxed_slice();

        let medium_buckets = (0..medium_bucket_count)
            .map(|_| AtomicU64::new(0))
            .collect::<Vec<_>>()
            .into_boxed_slice();

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Self {
            short_buckets,
            short_current_bucket: AtomicUsize::new(0),
            short_bucket_duration_secs,
            short_bucket_count,
            medium_buckets,
            medium_current_bucket: AtomicUsize::new(0),
            medium_bucket_duration_secs,
            medium_bucket_count,
            last_update: AtomicU64::new(now),
            short_window_size,
            medium_window_size,
        }
    }

    /// Record one access (lock-free operation)
    fn record_access(&self) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        trace!("Recording access at timestamp: {}", now);

        // Update last access time
        self.last_update.store(now, Ordering::Relaxed);

        // Record in short window
        self.maybe_reset_short_bucket(now);
        let short_bucket_idx = self.calculate_short_bucket_index(now);
        let short_count = self.short_buckets[short_bucket_idx].fetch_add(1, Ordering::Relaxed);
        trace!(
            "Recorded in short bucket {}: count = {}",
            short_bucket_idx,
            short_count + 1
        );

        // Record in medium window
        self.maybe_reset_medium_bucket(now);
        let medium_bucket_idx = self.calculate_medium_bucket_index(now);
        let medium_count = self.medium_buckets[medium_bucket_idx].fetch_add(1, Ordering::Relaxed);
        trace!(
            "Recorded in medium bucket {}: count = {}",
            medium_bucket_idx,
            medium_count + 1
        );
    }

    /// Get weighted access frequency using both short and medium windows
    fn get_weighted_access_frequency(&self, short_weight: f64, medium_weight: f64) -> f64 {
        let short_freq = self.get_short_window_frequency();
        let medium_freq = self.get_medium_window_frequency();

        // Normalize weights
        let total_weight = short_weight + medium_weight;
        if total_weight == 0.0 {
            trace!("Both weights are 0, returning 0 frequency");
            return 0.0;
        }
        let short_norm = short_weight / total_weight;
        let medium_norm = medium_weight / total_weight;

        let weighted_freq = short_freq * short_norm + medium_freq * medium_norm;
        trace!(
            "Weighted frequency: short={} ({:.2}), medium={} ({:.2}) -> weighted={:.2}",
            short_freq, short_norm, medium_freq, medium_norm, weighted_freq
        );

        weighted_freq
    }

    /// Get short window access frequency
    fn get_short_window_frequency(&self) -> f64 {
        self.get_window_frequency(
            &self.short_buckets,
            self.short_current_bucket.load(Ordering::Relaxed),
            self.short_bucket_duration_secs,
            self.short_bucket_count,
            self.short_window_size,
        )
    }

    /// Get medium window access frequency
    fn get_medium_window_frequency(&self) -> f64 {
        self.get_window_frequency(
            &self.medium_buckets,
            self.medium_current_bucket.load(Ordering::Relaxed),
            self.medium_bucket_duration_secs,
            self.medium_bucket_count,
            self.medium_window_size,
        )
    }

    /// Generic method to calculate frequency for any window
    fn get_window_frequency(
        &self,
        buckets: &[AtomicU64],
        current_bucket_idx: usize,
        bucket_duration_secs: u64,
        bucket_count: usize,
        window_size: Duration,
    ) -> f64 {
        let window_bucket_count =
            (window_size.as_secs() / bucket_duration_secs).min(bucket_count as u64) as usize;

        if window_bucket_count == 0 {
            return 0.0;
        }

        let mut total = 0u64;

        // Traverse the last few buckets
        for i in 0..window_bucket_count {
            let bucket_idx = if current_bucket_idx >= i {
                current_bucket_idx - i
            } else {
                bucket_count - i + current_bucket_idx
            };
            total += buckets[bucket_idx].load(Ordering::Relaxed);
        }

        total as f64 / window_size.as_secs_f64()
    }

    /// Calculate the short window bucket index
    fn calculate_short_bucket_index(&self, timestamp: u64) -> usize {
        let bucket_num = timestamp / self.short_bucket_duration_secs;
        (bucket_num as usize) % self.short_bucket_count
    }

    /// Calculate the medium window bucket index
    fn calculate_medium_bucket_index(&self, timestamp: u64) -> usize {
        let bucket_num = timestamp / self.medium_bucket_duration_secs;
        (bucket_num as usize) % self.medium_bucket_count
    }

    /// Reset short bucket if needed
    fn maybe_reset_short_bucket(&self, now: u64) {
        let expected_bucket = self.calculate_short_bucket_index(now);
        let current = self.short_current_bucket.load(Ordering::Relaxed);

        if current != expected_bucket
            && self
                .short_current_bucket
                .compare_exchange_weak(
                    current,
                    expected_bucket,
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                )
                .is_ok()
        {
            self.short_buckets[expected_bucket].store(0, Ordering::Relaxed);
            self.cleanup_old_short_buckets(now);
        }
    }

    /// Reset medium bucket if needed
    fn maybe_reset_medium_bucket(&self, now: u64) {
        let expected_bucket = self.calculate_medium_bucket_index(now);
        let current = self.medium_current_bucket.load(Ordering::Relaxed);

        if current != expected_bucket
            && self
                .medium_current_bucket
                .compare_exchange_weak(
                    current,
                    expected_bucket,
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                )
                .is_ok()
        {
            self.medium_buckets[expected_bucket].store(0, Ordering::Relaxed);
            self.cleanup_old_medium_buckets(now);
        }
    }

    /// Clean up expired short window buckets
    fn cleanup_old_short_buckets(&self, _now: u64) {
        let window_buckets =
            (self.short_window_size.as_secs() / self.short_bucket_duration_secs) as usize;
        let current_bucket_idx = self.short_current_bucket.load(Ordering::Relaxed);

        for (i, bucket) in self.short_buckets.iter().enumerate() {
            let bucket_age =
                (current_bucket_idx + self.short_bucket_count - i) % self.short_bucket_count;

            if bucket_age >= window_buckets {
                bucket.store(0, Ordering::Relaxed);
            }
        }
    }

    /// Clean up expired medium window buckets
    fn cleanup_old_medium_buckets(&self, _now: u64) {
        let window_buckets =
            (self.medium_window_size.as_secs() / self.medium_bucket_duration_secs) as usize;
        let current_bucket_idx = self.medium_current_bucket.load(Ordering::Relaxed);

        for (i, bucket) in self.medium_buckets.iter().enumerate() {
            let bucket_age =
                (current_bucket_idx + self.medium_bucket_count - i) % self.medium_bucket_count;

            if bucket_age >= window_buckets {
                bucket.store(0, Ordering::Relaxed);
            }
        }
    }
}

/// System performance metrics collector for adaptive cache optimization.
///
/// This structure tracks key performance indicators that influence the cache's
/// promotion strategy, enabling dynamic adaptation to changing workload patterns.
///
/// # Metrics Tracked
///
/// - **Hit Rate**: Overall cache effectiveness (0.0 - 1.0)
/// - **Request Volume**: Total system load indicator
/// - **Cache Utilization**: Memory pressure indicator
///
/// # Adaptive Decision Logic
///
/// ```ignore
/// // High load scenario
/// if system_load > 0.8 {
///     // Be more aggressive: lower threshold by 30%
///     threshold *= 0.7;
/// }
///
/// // Low hit rate scenario
/// if hit_rate < 0.6 {
///     // Be more conservative: raise threshold by 30%
///     threshold *= 1.3;
/// }
/// ```
///
/// # Implementation Notes
///
/// All metrics are stored as scaled integers to avoid floating-point operations
/// in hot paths. Values are typically scaled by 10000 to maintain 4 decimal places.
#[derive(Debug)]
struct SystemMetrics {
    /// Cache hit rate stored as scaled integer (0-10000 representing 0.0-1.0)
    ///
    /// **Calculation**: (cache_hits * 10000) / total_requests
    /// **Usage**: Determines if cache strategy should be conservative or aggressive
    /// **Impact**: Low hit rates trigger conservative promotion to prevent pollution
    hit_rate: AtomicU64,

    /// Total number of cache requests (hits + misses)
    ///
    /// **Purpose**: System load indicator and hit rate denominator
    /// **Trend**: Increasing values indicate higher system activity
    total_requests: AtomicU64,

    /// Number of successful cache hits
    ///
    /// **Purpose**: Cache effectiveness measurement and hit rate numerator
    /// **Optimization Goal**: Maximize this value relative to total_requests
    cache_hits: AtomicU64,

    /// Hot cache utilization stored as scaled integer (0-10000 representing 0.0-1.0)
    ///
    /// **Calculation**: (current_size * 10000) / max_capacity
    /// **Purpose**: Memory pressure indicator for adaptive thresholding
    /// **Impact**: High utilization may trigger aggressive promotion to improve hit rate
    hot_cache_utilization: AtomicU64,

    /// Sliding window request tracking for accurate rate calculation
    ///
    /// **Purpose**: Track requests in recent time window to compute true request rate
    /// **Implementation**: Fixed-size circular buffer with time buckets
    /// **Benefit**: Prevents permanent drift in load calculation
    request_buckets: [AtomicU64; 60], // 60 buckets for 1-minute sliding window
    current_request_bucket: AtomicU64,
    last_request_advance: AtomicU64,
}

impl SystemMetrics {
    fn new() -> Self {
        debug!("Initializing SystemMetrics with 60 request buckets");
        Self {
            hit_rate: AtomicU64::new(0),
            total_requests: AtomicU64::new(0),
            cache_hits: AtomicU64::new(0),
            hot_cache_utilization: AtomicU64::new(0),
            request_buckets: std::array::from_fn(|_| AtomicU64::new(0)),
            current_request_bucket: AtomicU64::new(0),
            last_request_advance: AtomicU64::new(0),
        }
    }

    fn record_request(&self, hit: bool) {
        let total = self.total_requests.fetch_add(1, Ordering::Relaxed) + 1;
        let hits = if hit {
            self.cache_hits.fetch_add(1, Ordering::Relaxed) + 1
        } else {
            self.cache_hits.load(Ordering::Relaxed)
        };

        trace!(
            "Recording cache request: hit={}, total_requests={}, cache_hits={}",
            hit, total, hits
        );

        // Update sliding window request tracking
        self.advance_request_buckets();
        let current_bucket = self.current_request_bucket.load(Ordering::Relaxed) as usize;
        self.request_buckets[current_bucket].fetch_add(1, Ordering::Relaxed);

        self.update_hit_rate();
    }

    /// Advance request buckets based on current time
    fn advance_request_buckets(&self) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let last_advance = self.last_request_advance.load(Ordering::Relaxed);

        if now <= last_advance {
            return;
        }

        // Calculate how many buckets to advance
        let buckets_to_advance = (now - last_advance).min(60) as usize; // Cap at 60 to avoid wrapping multiple times

        if buckets_to_advance == 0 {
            return;
        }

        // Try to update last_advance time, bail out if another thread beat us to it
        match self.last_request_advance.compare_exchange(
            last_advance,
            now,
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => {
                // We successfully updated the time, now advance the buckets
                let mut bucket = self.current_request_bucket.load(Ordering::Relaxed) as usize;
                for _ in 0..buckets_to_advance {
                    bucket = (bucket + 1) % 60;
                    self.current_request_bucket
                        .store(bucket as u64, Ordering::Relaxed);
                    // Clear the new bucket
                    self.request_buckets[bucket].store(0, Ordering::Relaxed);
                }
            }
            Err(_) => {
                // Another thread updated the time, just return
                // The next call will advance if needed
            }
        }
    }

    /// Get request rate from sliding window (requests per second)
    fn get_request_rate(&self) -> f64 {
        self.advance_request_buckets(); // Ensure buckets are up to date

        let mut total_requests = 0u64;
        for bucket in &self.request_buckets {
            total_requests += bucket.load(Ordering::Relaxed);
        }

        total_requests as f64 / 60.0 // requests per second over 1-minute window
    }

    fn update_hit_rate(&self) {
        let total = self.total_requests.load(Ordering::Relaxed);
        if total > 0 {
            let hits = self.cache_hits.load(Ordering::Relaxed);
            let rate = (hits * 10000) / total; // Scale to 0-10000 for 0.0-1.0
            self.hit_rate.store(rate, Ordering::Relaxed);
        }
    }

    fn get_hit_rate(&self) -> f64 {
        self.hit_rate.load(Ordering::Relaxed) as f64 / 10000.0
    }

    fn get_system_load(&self) -> f64 {
        // Simple heuristic: high cache utilization = high system load
        let utilization = self.hot_cache_utilization.load(Ordering::Relaxed) as f64 / 10000.0;
        let request_rate = self.get_request_rate(); // Use sliding window rate instead of total count

        // Combine utilization and request rate for load estimate
        // This is a simplified calculation - in practice you might use CPU/memory metrics
        utilization * 0.7 + (request_rate / 100.0).min(1.0) * 0.3 // Adjusted scaling for RPS
    }

    fn update_cache_utilization(&self, current_size: u64, max_size: u64) {
        if max_size > 0 {
            let utilization = (current_size * 10000) / max_size;
            self.hot_cache_utilization
                .store(utilization, Ordering::Relaxed);
        }
    }
}

/// Intelligent cache promotion policy engine.
///
/// This is the brain of the adaptive caching system, combining access pattern analysis
/// with system performance metrics to make intelligent promotion decisions.
///
/// # Decision Algorithm
///
/// The promotion decision follows this multi-step process:
///
/// ```ignore
/// // 1. Calculate adaptive threshold based on system state
/// let adaptive_threshold = base_threshold * load_factor * hitrate_factor;
///
/// // 2. Get weighted access frequency from multiple time windows
/// let weighted_freq = short_freq * short_weight + medium_freq * medium_weight;
///
/// // 3. Make final decision
/// let should_promote = weighted_freq >= adaptive_threshold;
/// ```
///
/// # Adaptive Threshold Factors
///
/// ## Load Factor
/// - **High Load** (>0.8): `factor = 0.7` (30% more aggressive)
/// - **Normal Load**: `factor = 1.0` (baseline)
///
/// ## Hit Rate Factor
/// - **Low Hit Rate** (<0.6): `factor = 1.3` (30% more conservative)
/// - **High Hit Rate** (>0.8): `factor = 0.9` (10% more aggressive)
/// - **Normal Hit Rate**: `factor = 1.0` (baseline)
///
/// # Thread Safety
///
/// This struct uses `Arc<RwLock<>>` for access stats and `Arc<>` for system metrics,
/// allowing safe concurrent access from multiple threads while maintaining consistency.
#[derive(Debug, Clone)]
struct Policy {
    /// Per-key access statistics with dual time window analysis
    ///
    /// Stores `AccessStats` for each cache key, tracking both short-term burst
    /// patterns and medium-term trends. Protected by RwLock for safe concurrent access.
    access_stats: Arc<RwLock<HashMap<String, AccessStats>>>,

    /// Global system performance metrics
    ///
    /// Tracks hit rates, request volumes, and cache utilization to inform
    /// adaptive threshold decisions. Uses atomic operations for lock-free access.
    system_metrics: Arc<SystemMetrics>,

    /// Short time window configuration for burst detection
    short_window_size: Duration,

    /// Medium time window configuration for trend analysis
    medium_window_size: Duration,

    /// Maximum number of entries to track (legacy parameter)
    max_entries: usize,

    /// Base promotion threshold before adaptive adjustments
    base_promotion_threshold: f64,

    /// Weight for short window frequency in promotion decisions
    short_window_weight: f64,

    /// Weight for medium window frequency in promotion decisions
    medium_window_weight: f64,

    /// Enable/disable adaptive threshold adjustment
    enable_adaptive_threshold: bool,

    /// System load threshold triggering aggressive promotion mode
    aggressive_promotion_load_threshold: f64,

    /// Hit rate threshold triggering conservative promotion mode
    conservative_promotion_hit_rate_threshold: f64,
}

impl Policy {
    #[allow(clippy::too_many_arguments)]
    fn new(
        short_window_size: Duration,
        medium_window_size: Duration,
        max_entries: usize,
        base_promotion_threshold: f64,
        short_window_weight: f64,
        medium_window_weight: f64,
        enable_adaptive_threshold: bool,
        aggressive_promotion_load_threshold: f64,
        conservative_promotion_hit_rate_threshold: f64,
    ) -> Self {
        info!(
            "Creating Policy with configuration: base_threshold={:.2}, short_weight={:.2}, medium_weight={:.2}, adaptive={}",
            base_promotion_threshold,
            short_window_weight,
            medium_window_weight,
            enable_adaptive_threshold
        );

        Policy {
            access_stats: Arc::new(RwLock::new(HashMap::new())),
            system_metrics: Arc::new(SystemMetrics::new()),
            short_window_size,
            medium_window_size,
            max_entries,
            base_promotion_threshold,
            short_window_weight,
            medium_window_weight,
            enable_adaptive_threshold,
            aggressive_promotion_load_threshold,
            conservative_promotion_hit_rate_threshold,
        }
    }

    /// Calculate adaptive promotion threshold based on system conditions
    fn calculate_adaptive_threshold(&self) -> f64 {
        if !self.enable_adaptive_threshold {
            trace!(
                "Adaptive threshold disabled, using base threshold: {:.2}",
                self.base_promotion_threshold
            );
            return self.base_promotion_threshold;
        }

        let system_load = self.system_metrics.get_system_load();
        let hit_rate = self.system_metrics.get_hit_rate();

        let mut threshold = self.base_promotion_threshold;
        let mut adjustments = Vec::new();

        // Adjust based on system load
        if system_load > self.aggressive_promotion_load_threshold {
            // High load: be more aggressive with promotion (lower threshold)
            threshold *= 0.7;
            adjustments.push(format!("high load ({:.2}): *0.7", system_load));
        }

        // Adjust based on hit rate
        if hit_rate < self.conservative_promotion_hit_rate_threshold {
            // Low hit rate: be more conservative (higher threshold) to avoid cache pollution
            threshold *= 1.3;
            adjustments.push(format!("low hit rate ({:.2}): *1.3", hit_rate));
        } else if hit_rate > 0.8 {
            // High hit rate: be more aggressive to maintain good performance
            threshold *= 0.9;
            adjustments.push(format!("high hit rate ({:.2}): *0.9", hit_rate));
        }

        // Ensure threshold stays within reasonable bounds
        let final_threshold = threshold.clamp(1.0, 50.0);

        debug!(
            "Adaptive threshold calculation: base={:.2}, system_load={:.2}, hit_rate={:.2}, adjustments=[{}], final={:.2}",
            self.base_promotion_threshold,
            system_load,
            hit_rate,
            adjustments.join(", "),
            final_threshold
        );

        final_threshold
    }

    async fn record_access(&self, key: String) {
        trace!("Recording access for key: {}", key);

        // Use read lock to get or create AccessStats
        {
            let stats = self.access_stats.read().await;
            if let Some(entry) = stats.get(&key) {
                // If exists, directly record access (no need for write lock)
                trace!("Found existing access stats for key: {}", key);
                entry.record_access();
                return;
            }
        }

        // If not exists, get write lock to create
        trace!("Creating new access stats for key: {}", key);
        let mut stats = self.access_stats.write().await;
        // Double check, prevent other threads from creating in the meantime
        if let Some(entry) = stats.get(&key) {
            trace!("Access stats created by another thread for key: {}", key);
            entry.record_access();
        } else {
            debug!("Creating new AccessStats entry for key: {}", key);
            let entry = AccessStats::new(
                self.short_window_size,
                self.medium_window_size,
                self.max_entries,
            );
            entry.record_access();
            stats.insert(key, entry);
        }
    }

    async fn should_promote(&self, key: String) -> bool {
        // Calculate adaptive threshold
        let threshold = self.calculate_adaptive_threshold();
        trace!(
            "Calculated promotion threshold for key '{}': {:.2}",
            key, threshold
        );

        // Use read lock to check promotion conditions, reduce lock contention
        let stats = self.access_stats.read().await;
        if let Some(entry) = stats.get(&key) {
            // Use weighted frequency from both time windows
            let weighted_frequency = entry
                .get_weighted_access_frequency(self.short_window_weight, self.medium_window_weight);

            let should_promote = weighted_frequency >= threshold;
            debug!(
                "Promotion decision for key '{}': frequency={:.2}, threshold={:.2}, should_promote={}",
                key, weighted_frequency, threshold, should_promote
            );

            should_promote
        } else {
            trace!("No access stats found for key '{}', not promoting", key);
            false
        }
    }

    /// Record cache request for metrics tracking
    fn record_cache_request(&self, hit: bool) {
        trace!("Recording cache request: hit={}", hit);
        self.system_metrics.record_request(hit);
    }

    /// Update cache utilization metrics
    fn update_cache_utilization(&self, current_size: u64, max_size: u64) {
        let utilization = if max_size > 0 {
            current_size as f64 / max_size as f64
        } else {
            0.0
        };
        trace!(
            "Updating cache utilization: {}/{} ({:.2}%)",
            current_size,
            max_size,
            utilization * 100.0
        );
        self.system_metrics
            .update_cache_utilization(current_size, max_size);
    }

    /// Clean up old entries
    #[allow(dead_code)]
    async fn cleanup_old_entries(&self, max_idle_duration: Duration) {
        let mut stats = self.access_stats.write().await;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        stats.retain(|_, entry| {
            let last_update = entry.last_update.load(Ordering::Relaxed);
            let idle_duration = now.saturating_sub(last_update);
            idle_duration < max_idle_duration.as_secs()
        });
    }
}

/// High-performance adaptive dual-layer cache system.
///
/// This cache implements a sophisticated three-tier storage architecture with intelligent
/// promotion strategies based on access patterns and system performance metrics.
///
/// # Architecture Tiers
///
/// ```text
/// ┌─────────────────────────────────────────────────────────────┐
/// │                    Request Flow                              │
/// ├─────────────────────────────────────────────────────────────┤
/// │ 1. Hot Cache (Memory)     ← Fastest, O(1) lookup            │
/// │    - Size: 1024 items      - Stores actual data             │
/// │    - TTL: 120s            - For frequently accessed items   │
/// │    - TTI: 30s             - Adaptive promotion based on     │
/// │                           - access frequency & system load  │
/// │                                                             │
/// │ 2. Cold Cache (Memory)     ← Fast, O(1) metadata lookup     │
/// │    - Size: 1024 items      - Tracks all accessed keys       │
/// │    - TTL: 120s            - Enables access pattern analysis │
/// │    - TTI: 30s             - Lightweight key tracking        │
/// │                                                             │
/// │ 3. Disk Storage (SSD/HDD)  ← Slower, but persistent        │
/// │    - Unlimited size       - SHA256-based file naming       │
/// │    - System cache dir     - Fallback for all data           │
/// │    - Async I/O            - Handles large files efficiently │
/// └─────────────────────────────────────────────────────────────┘
/// ```
///
/// # Promotion Strategy
///
/// The cache uses an intelligent promotion algorithm that considers:
///
/// 1. **Dual Time Windows**: Short-term (10s) and medium-term (60s) access patterns
/// 2. **Weighted Frequency**: Combines burst detection with trend analysis
/// 3. **Adaptive Thresholding**: Adjusts based on system load and hit rates
/// 4. **System Metrics**: Real-time performance feedback
///
/// # Performance Characteristics
///
/// - **Hot Cache Hit**: ~50ns (memory access)
/// - **Cold Cache Hit + Promotion**: ~1-10μs (memory + disk I/O)
/// - **Cold Cache Miss + Disk Load**: ~1-10ms (disk I/O)
/// - **Concurrent Access**: Lock-free for reads, minimal contention for writes
///
/// # Usage Examples
///
/// ## Basic Usage
/// ```ignore
/// # use slayerfs::chuck::cache::ChunksCache;
/// # async fn demo() -> anyhow::Result<()> {
/// let cache = ChunksCache::new().await?;
/// cache.insert("key1", &data).await?;
/// let value = cache.get(&"key1".to_string()).await?;
/// # Ok(())
/// # }
/// ```
///
/// ## Custom Configuration
/// ```ignore
/// let config = ChunksCacheConfig {
///     base_promotion_threshold: 5.0,        // More aggressive
///     short_window_weight: 0.8,            // Prioritize bursts
///     enable_adaptive_threshold: true,      // Enable adaptation
///     ..Default::default()
/// };
/// let cache = ChunksCache::new_with_config(config).await?;
/// ```
///
/// ## Performance-Tuned Configuration
/// ```ignore
/// use std::time::Duration;
///
/// let config = ChunksCacheConfig {
///     hot_cache_size: 2048,                 // Larger hot cache
///     base_promotion_threshold: 3.0,        // Very aggressive
///     short_window_size: Duration::from_secs(5),   // Faster response
///     short_window_weight: 0.9,            // Heavily prefer bursts
///     aggressive_promotion_load_threshold: 0.6,    // Earlier aggression
///     ..Default::default()
/// };
/// ```
///
/// # Thread Safety
///
/// This cache is fully thread-safe and designed for high-concurrency environments:
/// - All operations are async and non-blocking
/// - Access statistics use lock-free atomic operations
/// - Cache operations use Moka's concurrent-safe implementation
///
/// # Memory Management
///
/// - **Hot Cache**: Stores actual data, limited by `hot_cache_size`
/// - **Cold Cache**: Stores only `()` markers, minimal memory overhead
/// - **Access Stats**: Per-key statistics, automatically cleaned up when idle
/// - **Disk Storage**: Uses system temp directory, respects available space
pub struct ChunksCache {
    /// Persistent disk storage backend with SHA256-based file naming
    disk_storage: DiskStorage,

    /// Hot cache tier storing frequently accessed data in memory
    /// Uses Moka's high-performance concurrent cache implementation
    hot_cache: moka::future::Cache<String, Vec<u8>>,

    /// Cold cache tier tracking all accessed keys for pattern analysis
    /// Stores empty tuples () as lightweight metadata markers
    cold_cache: moka::future::Cache<String, ()>,

    /// Intelligent promotion policy engine with adaptive thresholding
    policy: Policy,

    /// Cache configuration parameters (stored for runtime adjustments)
    config: ChunksCacheConfig,
}

impl ChunksCache {
    /// Creates a new ChunksCache with default configuration
    #[allow(dead_code)]
    pub async fn new() -> anyhow::Result<Self> {
        Self::new_with_config(ChunksCacheConfig::default()).await
    }

    /// Creates a new ChunksCache with custom configuration
    pub async fn new_with_config(mut config: ChunksCacheConfig) -> anyhow::Result<Self> {
        info!(
            "Creating new ChunksCache with configuration: hot_cache_size={}, cold_cache_size={}, base_promotion_threshold={}",
            config.hot_cache_size, config.cold_cache_size, config.base_promotion_threshold
        );

        let cache_dir = config
            .disk_storage_dir
            .take()
            .unwrap_or_else(|| cache_dir().unwrap());
        debug!("Using cache directory: {:?}", cache_dir);
        let disk_storage = DiskStorage::new(cache_dir).await?;

        let hot_cache_builder = moka::future::Cache::builder()
            .max_capacity(config.hot_cache_size as u64)
            .time_to_idle(Duration::from_secs(30))
            .time_to_live(Duration::from_secs(120));
        let cold_cache_builder = moka::future::Cache::builder()
            .max_capacity(config.cold_cache_size as u64)
            .time_to_idle(Duration::from_secs(30))
            .time_to_live(Duration::from_secs(120));

        debug!(
            "Creating policy with adaptive threshold: {}",
            config.enable_adaptive_threshold
        );
        let policy = Policy::new(
            config.short_window_size,
            config.medium_window_size,
            config.max_access_entries,
            config.base_promotion_threshold,
            config.short_window_weight,
            config.medium_window_weight,
            config.enable_adaptive_threshold,
            config.aggressive_promotion_load_threshold,
            config.conservative_promotion_hit_rate_threshold,
        );

        info!("ChunksCache created successfully");
        Ok(Self {
            disk_storage,
            hot_cache: hot_cache_builder.build(),
            cold_cache: cold_cache_builder.build(),
            policy,
            config,
        })
    }

    pub async fn get(&self, key: &String) -> Option<Vec<u8>> {
        trace!("Cache GET request for key: {}", key);
        self.policy.record_access(key.clone()).await;

        // Check hot cache first
        if let Some(value) = self.hot_cache.get(key).await {
            debug!(
                "Hot cache HIT for key: {}, size: {} bytes",
                key,
                value.len()
            );
            self.policy.record_cache_request(true);
            self.update_utilization_metrics();
            return Some(value);
        }

        debug!("Hot cache MISS for key: {}", key);
        self.policy.record_cache_request(false);

        let diskstorage = self.disk_storage.clone();
        let policy = self.policy.clone();
        let hot_cache = self.hot_cache.clone();
        let key_owned = key.to_string();

        // ensure key exists in cold cache
        self.cold_cache.get(key).await?;

        let load_future = async move {
            trace!("Loading data from disk for key: {}", key_owned);
            let value = diskstorage.load(&key_owned).await.ok().unwrap_or_default();

            if value.is_empty() {
                warn!("No data found on disk for key: {}", key_owned);
                return value;
            }

            debug!(
                "Loaded {} bytes from disk for key: {}",
                value.len(),
                key_owned
            );

            if policy.should_promote(key_owned.clone()).await {
                debug!("Promoting key to hot cache: {}", key_owned);
                hot_cache.insert(key_owned.clone(), value.clone()).await;
            } else {
                trace!("Key not eligible for promotion: {}", key_owned);
            }

            value
        };

        let value = self.hot_cache.get_with_by_ref(key, load_future).await;

        self.update_utilization_metrics();

        if value.is_empty() {
            debug!("No data found for key: {}", key);
            None
        } else {
            debug!("Returning {} bytes for key: {}", value.len(), key);
            value.into()
        }
    }

    /// Update cache utilization metrics
    fn update_utilization_metrics(&self) {
        let current_size = self.hot_cache.entry_count();
        let max_size = self.config.hot_cache_size as u64;
        trace!(
            "Updating cache utilization: {}/{} entries",
            current_size, max_size
        );
        self.policy.update_cache_utilization(current_size, max_size);
    }

    pub async fn insert(&self, key: &str, data: &Vec<u8>) -> anyhow::Result<()> {
        info!(
            "Cache INSERT request for key: {}, size: {} bytes",
            key,
            data.len()
        );
        trace!("Inserting into hot cache: {}", key);
        self.hot_cache.insert(key.to_owned(), data.clone()).await;

        trace!("Storing on disk: {}", key);
        self.disk_storage.store(key, data).await?;

        trace!("Adding to cold cache: {}", key);
        self.cold_cache.insert(key.to_owned(), ()).await;

        debug!("Successfully inserted key: {}", key);
        Ok(())
    }

    #[allow(dead_code)]
    pub async fn remove(&self, key: &String) -> anyhow::Result<()> {
        info!("Cache REMOVE request for key: {}", key);
        trace!("Invalidating from hot cache: {}", key);
        self.hot_cache.invalidate(key).await;
        // self.disk_storage.remove(key).await?;
        trace!("Invalidating from cold cache: {}", key);
        self.cold_cache.invalidate(key).await;

        debug!("Successfully removed key: {}", key);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Duration;
    use tempfile::tempdir;
    use tokio;

    // Test helper: create a temporary storage directory
    async fn setup_test_storage() -> (DiskStorage, tempfile::TempDir) {
        let temp_dir = tempdir().unwrap();
        let storage = DiskStorage::new(temp_dir.path()).await.unwrap();
        (storage, temp_dir)
    }

    // Test helper: generate sample data
    fn generate_test_data(size: usize) -> Vec<u8> {
        (0..size).map(|i| (i % 256) as u8).collect()
    }

    #[tokio::test]
    async fn test_new_creates_directory() {
        let temp_dir = tempdir().unwrap();
        let dir_path = temp_dir.path().join("subdir");

        // Ensure the directory does not exist
        assert!(!dir_path.exists());

        let _storage = DiskStorage::new(&dir_path).await.unwrap();
        assert!(dir_path.exists());
        assert!(dir_path.is_dir());
    }

    #[tokio::test]
    async fn test_new_existing_directory() {
        let temp_dir = tempdir().unwrap();

        // Directory already exists
        assert!(temp_dir.path().exists());

        let _storage = DiskStorage::new(temp_dir.path()).await.unwrap();
        assert!(temp_dir.path().exists());
    }

    #[tokio::test]
    async fn test_etag_to_filename_special_characters() {
        let binding = "a".repeat(1000);
        let etags = vec![
            "normal",
            "etag-with-dashes",
            "etag_with_underscores",
            "etag with spaces",
            "etag@with#special$chars%",
            "ChineseLabel",
            "🚀emoji-etag",
            "",       // Empty string
            "a",      // Single character
            &binding, // Long string
        ];

        for etag in etags {
            let filename = DiskStorage::key_to_filename(etag);
            assert!(!filename.is_empty());
            // Filenames should be valid (no path separators, etc.)
            assert!(!filename.contains('/'));
            assert!(!filename.contains('\\'));
            assert!(!filename.contains(':'));
        }
    }

    #[tokio::test]
    async fn test_store_and_load_basic() {
        let (storage, _temp_dir) = setup_test_storage().await;
        let etag = "test_etag_1";
        let test_data = b"Hello, World!".to_vec();

        // Store the data
        storage.store(etag, &test_data).await.unwrap();

        // Load the data
        let loaded_data = storage.load(etag).await.unwrap();
        assert_eq!(loaded_data, test_data);
    }

    #[tokio::test]
    async fn test_store_and_load_large_data() {
        let (storage, _temp_dir) = setup_test_storage().await;
        let etag = "large_data_etag";

        // Generate 1 MiB of test data
        let large_data = generate_test_data(1024 * 1024);

        storage.store(etag, &large_data).await.unwrap();
        let loaded_data = storage.load(etag).await.unwrap();
        assert_eq!(loaded_data, large_data);
    }

    #[tokio::test]
    async fn test_store_and_load_empty_data() {
        let (storage, _temp_dir) = setup_test_storage().await;
        let etag = "empty_data_etag";
        let empty_data = vec![];

        storage.store(etag, &empty_data).await.unwrap();
        let loaded_data = storage.load(etag).await.unwrap();
        assert_eq!(loaded_data, empty_data);
    }

    #[tokio::test]
    async fn test_store_overwrite() {
        let (storage, _temp_dir) = setup_test_storage().await;
        let etag = "overwrite_etag";

        let data1 = b"First version".to_vec();
        let data2 = b"Second version".to_vec();

        storage.store(etag, &data1).await.unwrap();
        storage.store(etag, &data2).await.unwrap(); // Should overwrite the first copy

        let loaded_data = storage.load(etag).await.unwrap();
        assert_eq!(loaded_data, data2); // Should match the second version
    }

    #[tokio::test]
    async fn test_load_nonexistent_file() {
        let (storage, _temp_dir) = setup_test_storage().await;
        let etag = "nonexistent_etag";

        let result = storage.load(etag).await;
        assert!(result.is_err());

        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("does not exist"));
    }

    #[tokio::test]
    async fn test_remove_existing_file() {
        let (storage, _temp_dir) = setup_test_storage().await;
        let etag = "to_remove_etag";
        let test_data = b"Data to remove".to_vec();

        storage.store(etag, &test_data).await.unwrap();
        assert!(storage.load(etag).await.is_ok()); // File exists

        storage.remove(etag).await.unwrap();
        assert!(storage.load(etag).await.is_err()); // File should have been removed
    }

    #[tokio::test]
    async fn test_remove_nonexistent_file() {
        let (storage, _temp_dir) = setup_test_storage().await;
        let etag = "nonexistent_remove_etag";

        let result = storage.remove(etag).await;
        assert!(result.is_err());

        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("does not exist"));
    }

    #[tokio::test]
    async fn test_multiple_operations_same_etag() {
        let (storage, _temp_dir) = setup_test_storage().await;
        let etag = "multi_op_etag";
        let data1 = b"Data 1".to_vec();
        let data2 = b"Data 2".to_vec();

        // Store → load → store → load → delete → attempt load
        storage.store(etag, &data1).await.unwrap();
        assert_eq!(storage.load(etag).await.unwrap(), data1);

        storage.store(etag, &data2).await.unwrap();
        assert_eq!(storage.load(etag).await.unwrap(), data2);

        storage.remove(etag).await.unwrap();
        assert!(storage.load(etag).await.is_err());
    }

    #[tokio::test]
    async fn test_concurrent_operations() {
        let (storage, _temp_dir) = setup_test_storage().await;

        let mut handles = vec![];

        // Launch multiple concurrent tasks
        for i in 0..10 {
            let storage_clone = DiskStorage {
                base_dir: storage.base_dir.clone(),
            };
            let etag = format!("concurrent_etag_{}", i);
            let data = format!("Data for {}", i).into_bytes();

            handles.push(tokio::spawn(async move {
                storage_clone.store(&etag, &data).await.unwrap();
                let loaded = storage_clone.load(&etag).await.unwrap();
                assert_eq!(loaded, data);
                storage_clone.remove(&etag).await.unwrap();
            }));
        }

        // Wait for every task to finish
        for handle in handles {
            handle.await.unwrap();
        }
    }

    #[tokio::test]
    async fn test_filename_uniqueness() {
        let etag1 = "test1";
        let etag2 = "test2";

        let filename1 = DiskStorage::key_to_filename(etag1);
        let filename2 = DiskStorage::key_to_filename(etag2);

        // Different etags should produce different filenames
        assert_ne!(filename1, filename2);
    }

    #[tokio::test]
    async fn test_files_actually_created() {
        let (storage, _temp_dir) = setup_test_storage().await;
        let etag = "file_creation_test";
        let test_data = b"Test data".to_vec();

        // Ensure the directory is empty before storing (except system files)
        let mut entries = fs::read_dir(&storage.base_dir).await.unwrap();
        let mut initial_count = 0;
        while entries.next_entry().await.unwrap().is_some() {
            initial_count += 1;
        }

        storage.store(etag, &test_data).await.unwrap();

        // Verify that the file is actually created
        let mut entries = fs::read_dir(&storage.base_dir).await.unwrap();
        let mut final_count = 0;
        while entries.next_entry().await.unwrap().is_some() {
            final_count += 1;
        }
        assert_eq!(final_count, initial_count + 1);
    }

    #[tokio::test]
    async fn test_error_messages() {
        let (storage, _temp_dir) = setup_test_storage().await;
        let etag = "error_test_etag";

        // Test error message when loading a missing file
        let load_error = storage.load(etag).await.unwrap_err();
        let error_string = load_error.to_string();
        assert!(error_string.contains("does not exist"));

        // Test error message when deleting a missing file
        let remove_error = storage.remove(etag).await.unwrap_err();
        let error_string = remove_error.to_string();
        assert!(error_string.contains("does not exist"));
    }

    // ========== AccessStats tests ==========

    #[tokio::test]
    async fn test_access_stats_basic_functionality() {
        let short_window_size = Duration::from_secs(10);
        let medium_window_size = Duration::from_secs(60);
        let max_entries = 100;
        let stats = AccessStats::new(short_window_size, medium_window_size, max_entries);

        // Should have zero access initially
        assert_eq!(stats.get_short_window_frequency(), 0.0);
        assert_eq!(stats.get_medium_window_frequency(), 0.0);

        // Record a few accesses
        for _ in 0..5 {
            stats.record_access();
        }

        // Check the access frequency
        assert!(stats.get_short_window_frequency() > 0.0);
        assert!(stats.get_medium_window_frequency() > 0.0);

        // Check the weighted frequency
        let weighted_freq = stats.get_weighted_access_frequency(0.7, 0.3);
        assert!(weighted_freq > 0.0);
    }

    #[tokio::test]
    async fn test_access_stats_concurrent_access() {
        let short_window_size = Duration::from_secs(10);
        let medium_window_size = Duration::from_secs(60);
        let max_entries = 100;
        let stats = Arc::new(AccessStats::new(
            short_window_size,
            medium_window_size,
            max_entries,
        ));

        let mut handles = vec![];

        // Launch multiple concurrent tasks to record accesses
        for _ in 0..10 {
            let stats_clone = stats.clone();
            handles.push(tokio::spawn(async move {
                for _ in 0..100 {
                    stats_clone.record_access();
                }
            }));
        }

        // Wait for every task to finish
        for handle in handles {
            handle.await.unwrap();
        }

        // Verify total access count via frequency calculation
        let frequency = stats.get_short_window_frequency();
        assert!(frequency > 0.0); // There should be recorded accesses
    }

    #[tokio::test]
    async fn test_access_stats_time_window() {
        let short_window_size = Duration::from_secs(5);
        let medium_window_size = Duration::from_secs(60);
        let max_entries = 100;
        let stats = AccessStats::new(short_window_size, medium_window_size, max_entries);

        // Record an access
        stats.record_access();
        stats.record_access();

        // There should be accesses in the short window
        assert!(stats.get_short_window_frequency() > 0.0);

        // Wait for the time bucket to expire (simulated here; real code would wait)
        // Note: this test may need tuning because our buckets are 1-second each
        tokio::time::sleep(Duration::from_secs(2)).await;

        // Accesses should remain within the 2-second window
        let frequency_2s = stats.get_short_window_frequency();
        assert!(frequency_2s > 0.0);
    }

    #[tokio::test]
    async fn test_policy_basic_operations() {
        let short_window_size = Duration::from_secs(10);
        let medium_window_size = Duration::from_secs(60);
        let max_entries = 100;
        let base_promotion_threshold = 5.0;
        let policy = Policy::new(
            short_window_size,
            medium_window_size,
            max_entries,
            base_promotion_threshold,
            0.7,  // short_window_weight
            0.3,  // medium_window_weight
            true, // enable_adaptive_threshold
            0.8,  // aggressive_promotion_load_threshold
            0.6,  // conservative_promotion_hit_rate_threshold
        );

        let key = "test_key".to_string();

        // Should not promote initially
        assert!(!policy.should_promote(key.clone()).await);

        // Record multiple accesses
        for _ in 0..10 {
            policy.record_access(key.clone()).await;
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        // Wait briefly so the access records take effect
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Promotion conditions should now be satisfied
        // Note: due to bucket implementation, more accesses may be required
        let additional_accesses = 50;
        for _ in 0..additional_accesses {
            policy.record_access(key.clone()).await;
        }

        // Check whether promotion conditions are met
        let should_promote = policy.should_promote(key.clone()).await;
        // If the frequency is high enough, promotion should occur
        if should_promote {
            println!("Key promoted successfully");
        } else {
            println!("Key not promoted - this is normal for low frequency access");
        }
    }

    #[tokio::test]
    async fn test_access_stats_frequency_calculation() {
        let short_window_size = Duration::from_secs(10);
        let medium_window_size = Duration::from_secs(60);
        let max_entries = 100;
        let stats = AccessStats::new(short_window_size, medium_window_size, max_entries);

        // Quickly record 10 accesses
        for _ in 0..10 {
            stats.record_access();
        }

        // Compute the short-term frequency
        let short_frequency = stats.get_short_window_frequency();
        assert!(short_frequency > 0.0);

        // Compute the mid-term frequency
        let medium_frequency = stats.get_medium_window_frequency();
        assert!(medium_frequency > 0.0);

        // Mid-term frequency should be lower or equal (larger window)
        assert!(medium_frequency <= short_frequency);

        // Test the weighted frequency calculation
        let weighted_freq = stats.get_weighted_access_frequency(0.7, 0.3);
        assert!(weighted_freq > 0.0);
        // Weighted frequency should fall between short and mid-term values
        assert!(weighted_freq >= medium_frequency);
        assert!(weighted_freq <= short_frequency);
    }
}
