mod cache;
mod path_trie;
#[allow(dead_code)]
pub mod session;

use crate::chuck::SliceDesc;
use crate::meta::config::{CacheCapacity, CacheTtl};
use crate::meta::file_lock::{FileLockInfo, FileLockQuery, FileLockRange, FileLockType};
use crate::meta::layer::MetaLayer;
use crate::meta::store::{
    DirEntry, FileAttr, MetaError, MetaStore, OpenFlags, SetAttrFlags, SetAttrRequest,
    StatFsSnapshot,
};
use crate::meta::stores::{CacheInvalidationEvent, EtcdMetaStore, EtcdWatchWorker, WatchConfig};
use crate::vfs::fs::FileType;
use crate::vfs::handles::DirHandle;
use async_trait::async_trait;
use dashmap::DashMap;
use futures::stream;
use if_addrs::get_if_addrs;
use moka::future::Cache;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::time::Duration;
use std::{collections::HashSet, process};
use tokio::sync::{Mutex, mpsc};
use tracing::{debug, info, trace, warn};
use uuid::Uuid;

use crate::vfs::extract_ino_and_chunk_index;
use cache::InodeCache;
use chrono::Utc;
use hostname::get as get_hostname;
use path_trie::PathTrie;
use session::{SessionInfo, SessionManager};

const ROOT_INODE: i64 = 1;

/// Configuration options for `MetaClient` that correspond to the core metadata
/// behaviours implemented by the Go `baseMeta`. Only a minimal subset of
/// fields is supported for now; additional knobs can be added as the Rust
/// client gains feature parity.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct MetaClientOptions {
    /// Optional mount point string used for diagnostics and session payloads.
    pub mount_point: Option<String>,
    /// Interval used by the background session heartbeat task.
    pub session_heartbeat: Duration,
    /// When true, metadata mutating operations return `MetaError::NotSupported`.
    pub read_only: bool,
    /// Disable background maintenance tasks (reserved for future use).
    pub no_background_jobs: bool,
    /// When true, lookups fall back to case-insensitive matching similar to
    /// JuiceFS `CaseInsensi`.
    pub case_insensitive: bool,
    /// Maximum symlink follow depth (POSIX SYMLOOP_MAX).
    pub max_symlinks: usize,
    /// Batch attribute prefetch configuration
    pub batch_prefetch: BatchPrefetchConfig,
}

/// Configuration for batch attribute prefetching during opendir
#[derive(Debug, Clone)]
pub struct BatchPrefetchConfig {
    /// Enable batch prefetching
    pub enabled: bool,
    /// Batch size for each query (default: 200)
    pub batch_size: usize,
    /// Maximum concurrent batches (default: 3)
    pub max_concurrency: usize,
}

impl Default for BatchPrefetchConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            batch_size: 200,
            max_concurrency: 3,
        }
    }
}

impl BatchPrefetchConfig {
    /// Create optimized config for traditional databases like Postgres/sqlite
    pub fn for_database() -> Self {
        Self {
            enabled: true,
            batch_size: 500,
            max_concurrency: 5,
        }
    }

    /// Create optimized config for Redis
    pub fn for_redis() -> Self {
        Self {
            enabled: true,
            batch_size: 300,
            max_concurrency: 10,
        }
    }

    /// Create optimized config for Etcd
    pub fn for_etcd() -> Self {
        Self {
            enabled: true,
            batch_size: 100, // Etcd Txn limited to ~128 ops
            max_concurrency: 3,
        }
    }

    /// Automatically select optimal config based on backend store name
    pub fn for_store(store_name: &str) -> Self {
        match store_name {
            name if name.contains("database") => Self::for_database(),
            name if name.contains("redis") => Self::for_redis(),
            name if name.contains("etcd") => Self::for_etcd(),
            _ => Self::default(),
        }
    }
}

impl Default for MetaClientOptions {
    fn default() -> Self {
        Self {
            mount_point: None,
            session_heartbeat: DEFAULT_SESSION_HEARTBEAT,
            read_only: false,
            no_background_jobs: false,
            case_insensitive: false,
            max_symlinks: 40,
            batch_prefetch: BatchPrefetchConfig::default(),
        }
    }
}
const DEFAULT_SESSION_HEARTBEAT: Duration = Duration::from_secs(30);

/// Metadata client with intelligent caching
///
/// This client wraps a MetaStore and provides transparent caching for:
/// - Inode attributes (file metadata)
/// - Directory children (directory listings)
/// - Path-to-inode mappings (path resolution)
pub struct MetaClient<T: MetaStore> {
    store: Arc<T>,
    options: MetaClientOptions,
    root: AtomicI64,
    #[allow(dead_code)]
    umounting: AtomicBool,
    inode_cache: Arc<InodeCache>,
    /// it's absolute path.
    /// Used for quick lookups and invalidation
    path_cache: Cache<String, i64>,
    /// Path trie for efficient prefix-based invalidation
    /// Replaces the old flat inode_to_paths mapping with O(depth) operations
    path_trie: Arc<PathTrie>,
    /// Reverse index: inode -> paths (for quick lookup during invalidation)
    /// Kept separate from trie for O(1) inode-to-paths lookup
    /// it's absolute path.
    inode_to_paths: Arc<DashMap<i64, Vec<String>>>,

    /// Manages background session heartbeats when enabled by callers.
    #[allow(dead_code)]
    session_manager: Arc<SessionManager<T>>,

    /// Watch Worker for etcd cache invalidation (for now only used for etcd).
    /// TODO: Now that we use the watch worker to invalidate cache in real-time,
    /// may want to consider a more detailed data caching approach.
    #[allow(dead_code)]
    watch_worker: Option<Arc<EtcdWatchWorker>>,
}

impl<T: MetaStore + 'static> MetaClient<T> {
    /// Creates a new MetaClient with cache configuration.
    ///
    /// # Arguments
    ///
    /// * `store` - The underlying metadata storage implementation
    /// * `capacity` - Cache capacity configuration (inode and path)
    /// * `ttl` - Cache TTL (time-to-live) configuration
    ///
    /// # Returns
    ///
    /// A new `MetaClient` instance with initialized caches
    #[allow(dead_code)]
    pub fn new(store: Arc<T>, capacity: CacheCapacity, ttl: CacheTtl) -> Arc<Self> {
        Self::with_options(store, capacity, ttl, MetaClientOptions::default())
    }

    /// Creates a new `MetaClient` with cache configuration and additional
    /// behavioural options ported from the JuiceFS `baseMeta` implementation.
    pub fn with_options(
        store: Arc<T>,
        capacity: CacheCapacity,
        ttl: CacheTtl,
        mut options: MetaClientOptions,
    ) -> Arc<Self> {
        let store_name = store.name();
        // Always use the predefined configuration values.
        // TODO: Make the values configurable.
        options.batch_prefetch = BatchPrefetchConfig::for_store(store_name);
        debug!(
            "store_name: {} Batch prefetch config: size={}, concurrency={}",
            store_name, options.batch_prefetch.batch_size, options.batch_prefetch.max_concurrency
        );

        // Detect if this is an etcd backend and start Watch Worker
        let watch_worker = if options.no_background_jobs {
            None
        } else if let Some(etcd_store) = store.as_any().downcast_ref::<EtcdMetaStore>() {
            let client = etcd_store.get_client();

            // Create Watch Worker configuration
            // Watch all metadata keys
            // TODO: Consider watching only specific prefixes?
            // For now, watch everything for simplicity
            let config = WatchConfig {
                key_prefix: "".to_string(),
                event_buffer_size: 1000,
                debug: false,
            };

            let (mut worker, invalidation_rx) = EtcdWatchWorker::new(client, config);

            if let Err(e) = worker.start() {
                warn!("Failed to start Watch Worker: {}", e);
                None
            } else {
                debug!("Watch Worker started for etcd backend");

                // Start the invalidation handler after creating MetaClient
                let worker_arc = Arc::new(worker);
                let rx = Arc::new(Mutex::new(invalidation_rx));
                Some((worker_arc, rx))
            }
        } else {
            None
        };

        let root_ino = store.root_ino();

        // Create MetaClient
        let client = Arc::new(Self {
            store: store.clone(),
            options,
            root: AtomicI64::new(root_ino),
            umounting: AtomicBool::new(false),
            inode_cache: Arc::new(InodeCache::new(capacity.inode as u64, ttl.inode_ttl)),
            path_cache: Cache::builder()
                .max_capacity(capacity.path as u64)
                .time_to_live(ttl.path_ttl)
                .build(),
            path_trie: Arc::new(PathTrie::new()),
            inode_to_paths: Arc::new(DashMap::new()),
            session_manager: Arc::new(SessionManager::new(store.clone())),
            watch_worker: watch_worker.as_ref().map(|(w, _)| w.clone()),
        });

        // Start cache invalidation handler if Watch Worker is active
        if let Some((_, rx)) = watch_worker.clone() {
            let client_clone = client.clone();
            tokio::spawn(async move {
                client_clone.handle_cache_invalidation(rx).await;
            });
        }

        if !client.options.read_only && !client.options.no_background_jobs {
            let client_clone = client.clone();
            tokio::spawn(async move {
                if let Err(err) = client_clone.start_default_session().await {
                    warn!("MetaClient: failed to auto-start session: {err}");
                }
            });
        }

        client
    }

    /// Returns the current root inode honoured by the client. This mirrors
    /// `baseMeta.root` which may differ from the physical root after `chroot`.
    pub fn root(&self) -> i64 {
        self.root.load(Ordering::SeqCst)
    }

    /// Returns the client options used when constructing this instance.
    #[allow(dead_code)]
    pub fn options(&self) -> &MetaClientOptions {
        &self.options
    }

    /// Returns a clone of the underlying raw `MetaStore` handle.
    /// Update the logical root inode. All subsequent metadata lookups treat
    /// `ROOT_INODE` as an alias for `inode`.
    #[allow(dead_code)]
    pub fn chroot(&self, inode: i64) {
        self.root.store(inode, Ordering::SeqCst);
    }

    fn check_root(&self, inode: i64) -> i64 {
        match inode {
            0 => ROOT_INODE,
            ROOT_INODE => self.root(),
            _ => inode,
        }
    }

    fn ensure_writable(&self) -> Result<(), MetaError> {
        if self.options.read_only {
            Err(MetaError::NotSupported(
                "metadata client configured read-only".to_string(),
            ))
        } else {
            Ok(())
        }
    }

    fn ensure_background_jobs(&self) -> Result<(), MetaError> {
        if self.options.no_background_jobs {
            Err(MetaError::NotSupported(
                "background jobs disabled".to_string(),
            ))
        } else {
            Ok(())
        }
    }

    fn mark_umounting(&self) {
        self.umounting.store(true, Ordering::SeqCst);
    }

    fn clear_umounting(&self) {
        self.umounting.store(false, Ordering::SeqCst);
    }

    fn is_umounting(&self) -> bool {
        self.umounting.load(Ordering::SeqCst)
    }
    /// Starts a background heartbeat session with the underlying store.
    ///
    /// Callers provide a `SessionInfo` struct containing session parameters understood by the backend;
    /// the client will register or update the session and then begin periodic heartbeats.
    #[allow(dead_code)]
    pub async fn start_session(&self, session_info: SessionInfo) -> Result<(), MetaError> {
        if self.options.read_only {
            info!("MetaClient: read-only mode, skipping session start");
            return Ok(());
        }
        self.ensure_background_jobs()?;
        self.clear_umounting();
        let session_manager = self.session_manager.clone();
        session_manager.start(session_info).await
    }

    /// Builds a default session payload and starts the heartbeat task.
    #[allow(dead_code)]
    pub async fn start_default_session(&self) -> Result<(), MetaError> {
        let payload = self.build_session_payload()?;
        self.start_session(payload).await
    }

    /// Stops the background heartbeat session if it was previously started.
    #[allow(dead_code)]
    pub async fn shutdown_session(&self) {
        self.mark_umounting();
        self.session_manager.shutdown().await;
    }

    /// Get the current session ID if a session is active.
    #[allow(dead_code)]
    pub async fn session_id(&self) -> Option<Uuid> {
        *self.session_manager.session_id.read().await
    }

    /// Get the current process ID.
    #[allow(dead_code)]
    pub fn process_id(&self) -> u32 {
        std::process::id()
    }

    /// Finds and removes stale sessions using store-provided helpers.
    ///
    /// Returns the number of sessions successfully cleaned. Failures are
    /// logged and skipped to keep the maintenance loop best-effort.
    #[allow(dead_code)]
    fn build_session_payload(&self) -> Result<SessionInfo, MetaError> {
        let host_name = get_hostname()
            .map_err(MetaError::from)?
            .into_string()
            .unwrap_or_else(|_| "unknown-host".to_string());
        let ip_addrs = Self::collect_local_ip_addrs()?;

        Ok(SessionInfo {
            version: env!("CARGO_PKG_VERSION").to_string(),
            host_name,
            ip_addrs,
            mount_point: self.options.mount_point.clone(),
            mount_time: Utc::now(),
            process_id: process::id(),
            created_at: Utc::now(),
        })
    }

    #[allow(dead_code)]
    fn collect_local_ip_addrs() -> Result<Vec<String>, MetaError> {
        let interfaces = get_if_addrs().map_err(MetaError::from)?;
        let mut addrs = HashSet::new();

        for iface in interfaces {
            let ip = iface.ip();
            if !ip.is_loopback() {
                addrs.insert(ip.to_string());
            }
        }

        let mut addrs: Vec<String> = addrs.into_iter().collect();
        addrs.sort();
        Ok(addrs)
    }
    /// Handle cache invalidation events from Watch Worker
    ///
    /// This runs in a background task and processes events from etcd Watch Worker
    /// to maintain cache consistency across multiple clients.
    async fn handle_cache_invalidation(
        self: Arc<Self>,
        rx: Arc<Mutex<mpsc::Receiver<CacheInvalidationEvent>>>,
    ) {
        let mut rx = rx.lock().await;

        info!("Cache invalidation handler started");

        while let Some(event) = rx.recv().await {
            if self.is_umounting() {
                break;
            }
            match event {
                CacheInvalidationEvent::InvalidateInode(ino) => {
                    self.inode_cache.invalidate_inode(ino).await;

                    if let Some(paths_entry) = self.inode_to_paths.get(&ino) {
                        for path in paths_entry.value() {
                            self.path_cache.invalidate(path).await;
                        }
                    }
                }

                CacheInvalidationEvent::InvalidateParentChildren(parent_ino) => {
                    self.invalidate_parent_path(parent_ino).await;
                }
                CacheInvalidationEvent::AddChild {
                    parent_ino,
                    name,
                    child_ino,
                } => {
                    self.inode_cache
                        .add_child(parent_ino, name, child_ino)
                        .await;
                    self.invalidate_parent_path(parent_ino).await;
                }

                CacheInvalidationEvent::RemoveChild { parent_ino, name } => {
                    self.inode_cache.remove_child(parent_ino, &name).await;
                    self.invalidate_parent_path(parent_ino).await;
                }

                CacheInvalidationEvent::UpdateInodeMetadata { ino, metadata } => {
                    self.inode_cache.update_metadata(ino, metadata).await;
                }

                CacheInvalidationEvent::UpdateChildren {
                    parent_ino,
                    children,
                } => {
                    self.inode_cache
                        .replace_children(parent_ino, children)
                        .await;
                    self.invalidate_parent_path(parent_ino).await;
                }
            }
        }

        info!("Cache invalidation handler stopped (channel closed)");
    }

    /// Preloads commonly accessed cache entries after operations that might benefit from caching.
    /// This helps maintain cache consistency and improves performance for subsequent operations.
    async fn preload_cache_entries(&self, inodes: &[i64]) -> Result<(), MetaError> {
        for &ino in inodes {
            // Preload inode attributes if not already cached
            if self.inode_cache.get_attr(ino).await.is_none()
                && let Ok(Some(attr)) = self.store.stat(ino).await
            {
                // For single-link files, we can cache the parent relationship
                // For multi-link files, parent is stored in LinkParentMeta
                let cache_parent = if attr.nlink <= 1 {
                    // Try to find parent from ContentMeta (best effort)
                    // This is a performance optimization - not critical for correctness
                    None // We'll let it be resolved lazily
                } else {
                    None // Multi-link files don't cache parent
                };

                self.inode_cache.insert_node(ino, attr, cache_parent).await;
            }
        }
        Ok(())
    }

    /// Intelligently invalidates path cache entries for a parent directory.
    ///
    /// # Strategy (Trie-based approach)
    ///
    /// When a modification occurs (create/delete/rename), we:
    /// 1. Find all paths that resolve to this parent inode (O(1) using reverse index)
    /// 2. For each path, remove its entire subtree from the trie (O(depth))
    /// 3. Invalidate all affected paths from the path cache
    /// 4. Clean up the reverse index for all removed paths
    ///
    /// # Arguments
    ///
    /// * `parent_ino` - The parent directory inode that was modified
    async fn invalidate_parent_path(&self, parent_ino: i64) {
        let parent_ino = self.check_root(parent_ino);
        // Step 1: Get all paths that resolve to this parent inode (O(1))
        if let Some(entry) = self.inode_to_paths.get(&parent_ino) {
            let paths = entry.value().clone();
            drop(entry);

            // Step 2: Remove each path and its descendants from the trie
            for parent_path in &paths {
                // Remove from trie - this automatically removes all child paths
                // E.g., removing "/a/b" also removes "/a/b/c", "/a/b/d", etc.
                // Returns Vec<(String, Vec<i64>)> with path and inodes BEFORE deletion
                let removed_info = self.path_trie.remove_by_prefix(parent_path).await;

                // Step 3: Invalidate all removed paths from Moka cache and clean up reverse index
                for (removed_path, inodes) in &removed_info {
                    // Invalidate from path cache
                    self.path_cache.invalidate(removed_path).await;

                    // Step 4: Clean up reverse index for all removed paths
                    // This fixes the memory leak where child path entries weren't cleaned up
                    for ino in inodes {
                        // Remove this specific path from the inode's path list
                        if let Some(mut entry) = self.inode_to_paths.get_mut(ino) {
                            entry.retain(|p| p != removed_path);
                            // If no more paths point to this inode, remove the entry
                            if entry.is_empty() {
                                drop(entry);
                                self.inode_to_paths.remove(ino);
                            }
                        }
                    }
                }
            }

            // Clean up the parent's reverse index entry
            self.inode_to_paths.remove(&parent_ino);
        } else {
            // Fallback: if we don't have reverse mapping, invalidate all
            // This maintains correctness even if the reverse mapping is incomplete
            self.path_cache.invalidate_all();
        }
    }

    /// Normalizes a path by resolving `.` and `..` components.
    ///
    /// # Arguments
    ///
    /// * `path` - The path to normalize (can be absolute or relative)
    ///
    /// # Returns
    ///
    /// A normalized absolute path with `.` and `..` resolved.
    fn normalize_path(path: &str) -> String {
        let mut components: Vec<&str> = Vec::new();
        let is_absolute = path.starts_with('/');

        for part in path.split('/') {
            match part {
                "" | "." => continue, // Skip empty and current directory
                ".." => {
                    if !(components.is_empty()) {
                        components.pop();
                    }
                }
                _ => components.push(part),
            }
        }

        if is_absolute {
            if components.is_empty() {
                "/".to_string()
            } else {
                format!("/{}", components.join("/"))
            }
        } else {
            components.join("/")
        }
    }

    /// Resolves a file path to its corresponding inode number (**lstat semantics**).
    ///
    /// This method walks through the path components from root to leaf,
    /// utilizing both inode cache and path cache for performance optimization.
    /// When encountering a symlink in an intermediate path component,
    /// it follows the symlink to resolve the target path.
    ///
    /// # Arguments
    ///
    /// * `path` - The absolute path to resolve (must start with '/')
    ///
    /// # Returns
    ///
    /// * `Ok(i64)` - The inode number of the file/directory/symlink
    /// * `Err(MetaError::NotFound)` - If any component in the path doesn't exist
    /// * `Err(MetaError::...)` - Other metadata errors
    pub async fn resolve_path(&self, path: &str) -> Result<i64, MetaError> {
        self.resolve_path_impl(path, false).await
    }
    /// Resolves a file path to its corresponding inode number (**stat semantics**).
    ///
    /// This method is similar to [`resolve_path`], but follows all symlinks
    /// including the final path component.
    #[allow(dead_code)]
    pub async fn resolve_path_follow(&self, path: &str) -> Result<i64, MetaError> {
        self.resolve_path_impl(path, true).await
    }

    /// Internal implementation of path resolution with configurable symlink behavior.
    ///
    /// # Arguments
    ///
    /// * `path` - The absolute path to resolve
    /// * `follow_final` - If true, follow stat semantics, false for lstat semantics
    async fn resolve_path_impl(&self, path: &str, follow_final: bool) -> Result<i64, MetaError> {
        trace!("MetaClient: Resolving path: {}", path);

        let root = self.root();
        if path == "/" {
            return Ok(root);
        }

        if let Some(ino) = self.path_cache.get(path).await {
            if !follow_final {
                trace!("MetaClient: Path cache HIT for '{}' -> inode {}", path, ino);
                return Ok(ino);
            }

            match self.cached_stat(ino).await {
                Ok(Some(attr)) if attr.kind == FileType::Symlink => {
                    info!(
                        "MetaClient: Path cache HIT for '{}' -> symlink inode {}, need to follow",
                        path, ino
                    );
                }

                _ => {
                    trace!("MetaClient: Path cache HIT for '{}' -> inode {}", path, ino);
                    return Ok(ino);
                }
            }
        }

        trace!("MetaClient: Path cache MISS for '{}'", path);

        let mut current_path = path.to_string();
        let mut symlink_depth = 0;
        let max_symlinks = self.options.max_symlinks;

        loop {
            if symlink_depth >= max_symlinks {
                return Err(MetaError::TooManySymlinks);
            }
            let segments: Vec<&str> = current_path
                .trim_start_matches('/')
                .split('/')
                .filter(|s| !s.is_empty())
                .collect();

            let segment_count = segments.len();
            let mut current_ino = root;
            let mut symlink_encountered = false;

            for (idx, seg) in segments.iter().enumerate() {
                let child_ino = self
                    .cached_lookup(current_ino, seg)
                    .await?
                    .ok_or_else(|| MetaError::NotFound(current_ino))?;

                let is_tail = idx == segment_count - 1;
                let should_follow = !is_tail || follow_final;

                // Follow symlinks based on position and follow_final flag
                if should_follow
                    && let Ok(Some(attr)) = self.cached_stat(child_ino).await
                    && attr.kind == FileType::Symlink
                {
                    info!(
                        "MetaClient: Following symlink at segment {} (inode {})",
                        seg, child_ino
                    );

                    let target = self.store.read_symlink(child_ino).await?;
                    let remaining = segments[idx + 1..].join("/");

                    // Resolve absolute vs relative target
                    let resolved_target = if target.starts_with('/') {
                        target
                    } else {
                        let parent_path = self
                            .get_paths(current_ino)
                            .await?
                            .into_iter()
                            .next()
                            .unwrap_or_else(|| "/".to_string());
                        if parent_path == "/" {
                            format!("/{}", target)
                        } else {
                            format!("{}/{}", parent_path, target)
                        }
                    };

                    current_path = if remaining.is_empty() {
                        Self::normalize_path(&resolved_target)
                    } else {
                        Self::normalize_path(&format!("{}/{}", resolved_target, remaining))
                    };

                    symlink_encountered = true;
                    symlink_depth += 1;
                    break;
                }

                current_ino = child_ino;
            }

            // If no symlink was encountered, we're done
            if !symlink_encountered {
                self.path_cache.insert(path.to_string(), current_ino).await;
                self.path_trie.insert(path, current_ino).await;
                self.inode_to_paths
                    .entry(current_ino)
                    .or_default()
                    .push(path.to_string());

                return Ok(current_ino);
            }
        }
    }

    /// Retrieves file attributes (metadata) for a given inode with caching.
    ///
    /// This is a cache-aware wrapper around the underlying store's stat operation.
    ///
    /// # Arguments
    ///
    /// * `ino` - The inode number to query
    ///
    /// # Returns
    ///
    /// * `Ok(Some(FileAttr))` - The file attributes if the inode exists
    /// * `Ok(None)` - If the inode doesn't exist
    /// * `Err(MetaError)` - On storage errors
    async fn cached_stat(&self, ino: i64) -> Result<Option<FileAttr>, MetaError> {
        let inode = self.check_root(ino);
        info!("MetaClient: stat request for inode {}", inode);

        if let Some(attr) = self.inode_cache.get_attr(inode).await {
            trace!("MetaClient: Inode cache HIT for inode {}", inode);
            return Ok(Some(attr));
        }

        trace!("MetaClient: Inode cache MISS for inode {}", inode);

        let attr = self.store.stat(inode).await?;

        if let Some(ref a) = attr {
            info!("MetaClient: Caching attr for inode {}", inode);
            self.inode_cache.insert_node(inode, a.clone(), None).await;
        }

        Ok(attr)
    }

    /// Looks up a child entry by name within a parent directory with caching.
    ///
    /// This is a cache-aware wrapper around the underlying store's lookup operation.
    ///
    /// # Arguments
    ///
    /// * `parent` - The inode number of the parent directory
    /// * `name` - The name of the child entry to look up
    ///
    /// # Returns
    ///
    /// * `Ok(Some(i64))` - The inode number of the child entry if found
    /// * `Ok(None)` - If no entry with the given name exists in the parent
    /// * `Err(MetaError)` - On storage errors
    async fn cached_lookup(&self, parent: i64, name: &str) -> Result<Option<i64>, MetaError> {
        let parent = self.check_root(parent);
        info!("MetaClient: lookup request for ({}, '{}')", parent, name);

        if let Some(ino) = self.inode_cache.lookup(parent, name).await {
            info!(
                "MetaClient: Inode cache HIT for ({}, '{}') -> inode {}",
                parent, name, ino
            );
            return Ok(Some(ino));
        }

        debug!("MetaClient: Inode cache MISS for ({}, '{}')", parent, name);

        let result = self.store.lookup(parent, name).await?;

        if let Some(ino) = result {
            info!(
                "MetaClient: Caching lookup result ({}, '{}') -> inode {}",
                parent, name, ino
            );
            if let Ok(Some(attr)) = self.store.stat(ino).await {
                let cache_parent = matches!(attr.kind, FileType::Dir).then_some(parent);

                self.inode_cache.insert_node(ino, attr, cache_parent).await;
            }
            self.inode_cache
                .add_child(parent, name.to_string(), ino)
                .await;
            Ok(result)
        } else if self.options.case_insensitive {
            self.resolve_case(parent, name).await
        } else {
            Ok(None)
        }
    }

    async fn resolve_case(&self, parent: i64, name: &str) -> Result<Option<i64>, MetaError> {
        let entries = self.store.readdir(parent).await?;
        for entry in entries {
            if entry.name.eq_ignore_ascii_case(name) {
                if let Ok(Some(attr)) = self.store.stat(entry.ino).await {
                    let cache_parent = matches!(attr.kind, FileType::Dir).then_some(parent);

                    self.inode_cache
                        .insert_node(entry.ino, attr, cache_parent)
                        .await;
                }
                self.inode_cache
                    .add_child(parent, entry.name.clone(), entry.ino)
                    .await;
                return Ok(Some(entry.ino));
            }
        }
        Ok(None)
    }

    /// Batch prefetch attributes for directory entries in background
    ///
    /// This method starts a background task that:
    /// 1. Collects inodes that need prefetching
    /// 2. Splits them into batches
    /// 3. Queries each batch concurrently
    /// 4. Inserts results into cache
    ///
    /// Returns a tuple of (done_flag, task_handle)
    pub fn spawn_batch_prefetch(
        &self,
        ino: i64,
        entries: &[DirEntry],
    ) -> (Arc<AtomicBool>, tokio::task::JoinHandle<()>) {
        let config = self.options.batch_prefetch.clone();

        if !config.enabled || entries.is_empty() {
            let done = Arc::new(std::sync::atomic::AtomicBool::new(true));
            let handle = tokio::spawn(async {});
            return (done, handle);
        }

        // Collect inodes that need to be fetched
        let inodes_to_fetch: Vec<i64> = entries.iter().map(|e| e.ino).collect();

        let batch_size = config.batch_size;
        let max_concurrency = config.max_concurrency;
        let done_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let done_flag_clone = Arc::clone(&done_flag);

        let store = Arc::clone(&self.store);
        let inode_cache = Arc::clone(&self.inode_cache);
        let parent_ino = ino; // Capture parent directory inode for the async block

        let task = tokio::spawn(async move {
            let start = std::time::Instant::now();
            debug!(
                "Starting batch prefetch for directory inode {}: {} entries, batch_size={}, max_concurrency={}",
                parent_ino,
                inodes_to_fetch.len(),
                batch_size,
                max_concurrency
            );

            // Split into batches
            let chunks: Vec<Vec<i64>> = inodes_to_fetch
                .chunks(batch_size)
                .map(|chunk| chunk.to_vec())
                .collect();

            let total_batches = chunks.len();

            // Process batches with controlled concurrency using stream
            // This is a single-layer spawn - abort will properly cancel all work
            use futures::stream::StreamExt;
            stream::iter(chunks.into_iter().enumerate())
                .map(|(batch_idx, chunk)| {
                    let store = Arc::clone(&store);
                    let inode_cache = Arc::clone(&inode_cache);
                    async move {
                        let batch_start = std::time::Instant::now();
                        match store.batch_stat(&chunk).await {
                            Ok(attrs) => {
                                let mut cached_count = 0;
                                // Insert results into cache
                                for (child_ino, attr_opt) in chunk.iter().zip(attrs.iter()) {
                                    if let Some(attr) = attr_opt {
                                        inode_cache
                                            .insert_node(*child_ino, attr.clone(), None)
                                            .await;
                                        cached_count += 1;
                                    }
                                }
                                debug!(
                                    "Batch {}/{} completed: {} inodes queried, {} cached in {:?}",
                                    batch_idx + 1,
                                    total_batches,
                                    chunk.len(),
                                    cached_count,
                                    batch_start.elapsed()
                                );
                            }
                            Err(e) => {
                                warn!(
                                    "Batch {}/{} failed: {} - continuing with remaining batches",
                                    batch_idx + 1,
                                    total_batches,
                                    e
                                );
                            }
                        }
                    }
                })
                .buffer_unordered(max_concurrency)
                .collect::<Vec<_>>()
                .await;

            debug!(
                "Prefetch completed for directory inode {}: {} total inodes in {:?}",
                parent_ino,
                inodes_to_fetch.len(),
                start.elapsed()
            );

            done_flag_clone.store(true, Ordering::Release);
        });

        (done_flag, task)
    }
}

#[async_trait]
#[allow(dead_code)]
impl<T: MetaStore + 'static> MetaLayer for MetaClient<T> {
    fn name(&self) -> &'static str {
        self.store.name()
    }

    fn root_ino(&self) -> i64 {
        self.root()
    }

    fn chroot(&self, inode: i64) {
        MetaClient::chroot(self, inode);
    }

    async fn initialize(&self) -> Result<(), MetaError> {
        self.store.initialize().await
    }

    async fn stat_fs(&self) -> Result<StatFsSnapshot, MetaError> {
        self.store.stat_fs().await
    }

    async fn stat(&self, ino: i64) -> Result<Option<FileAttr>, MetaError> {
        self.cached_stat(ino).await
    }

    async fn stat_fresh(&self, ino: i64) -> Result<Option<FileAttr>, MetaError> {
        let inode = self.check_root(ino);
        self.inode_cache.invalidate_inode(inode).await;

        let attr = self.store.stat(inode).await?;
        if let Some(ref a) = attr {
            self.inode_cache.insert_node(inode, a.clone(), None).await;
        }
        Ok(attr)
    }

    async fn lookup(&self, parent: i64, name: &str) -> Result<Option<i64>, MetaError> {
        self.cached_lookup(parent, name).await
    }

    async fn lookup_path(&self, path: &str) -> Result<Option<(i64, FileType)>, MetaError> {
        let ino = match self.resolve_path(path).await {
            Ok(ino) => ino,
            Err(MetaError::NotFound(_)) => return Ok(None),
            Err(e) => return Err(e),
        };

        let attr = self
            .cached_stat(ino)
            .await?
            .ok_or(MetaError::NotFound(ino))?;

        Ok(Some((ino, attr.kind)))
    }

    async fn readdir(&self, ino: i64) -> Result<Vec<DirEntry>, MetaError> {
        let inode = self.check_root(ino);
        info!("MetaClient: readdir request for inode {}", inode);

        if let Some(entries) = self.inode_cache.readdir(inode).await {
            info!(
                "MetaClient: Inode cache HIT for readdir inode {} ({} entries)",
                inode,
                entries.len()
            );
            return Ok(entries);
        }

        trace!("MetaClient: Inode cache MISS for readdir inode {}", inode);

        let mut entries = self.store.readdir(inode).await?;
        // Sort once before caching so readops always return stable ordering by name.
        entries.sort_by(|a, b| a.name.cmp(&b.name));

        info!(
            "MetaClient: Caching readdir result for inode {} ({} entries)",
            inode,
            entries.len()
        );

        // Ensure parent directory node is in cache before loading children
        self.inode_cache
            .ensure_node_in_cache(inode, &*self.store, None)
            .await?;

        // Load all children from database into cache, replacing any stale data
        let children_data: Vec<(String, i64)> =
            entries.iter().map(|e| (e.name.clone(), e.ino)).collect();
        self.inode_cache.load_children(inode, children_data).await;

        // Note: We shouldn't pre-fetch attributes here; use batch prefetch instead.
        Ok(entries)
    }

    async fn opendir(&self, ino: i64) -> Result<DirHandle, MetaError> {
        let inode = self.check_root(ino);
        let attr = self
            .cached_stat(inode)
            .await?
            .ok_or(MetaError::NotFound(inode))?;
        if attr.kind != FileType::Dir {
            return Err(MetaError::NotDirectory(inode));
        }

        let entries = self.readdir(inode).await?;
        let (done_flag, prefetch_task) = MetaClient::spawn_batch_prefetch(self, inode, &entries);
        Ok(DirHandle::with_prefetch_task(
            inode,
            entries,
            prefetch_task,
            done_flag,
        ))
    }

    async fn mkdir(&self, parent: i64, name: String) -> Result<i64, MetaError> {
        self.ensure_writable()?;
        let parent = self.check_root(parent);
        info!("MetaClient: mkdir operation for ({}, '{}')", parent, name);

        let ino = self.store.mkdir(parent, name.clone()).await?;

        debug!("MetaClient: mkdir created inode {}, updating cache", ino);

        // Ensure parent node is in cache
        self.inode_cache
            .ensure_node_in_cache(parent, &self.store, None)
            .await?;

        // Cache the new directory node
        if let Ok(Some(attr)) = self.store.stat(ino).await {
            self.inode_cache.insert_node(ino, attr, Some(parent)).await;
        }
        self.inode_cache.add_child(parent, name, ino).await;

        self.invalidate_parent_path(parent).await;

        Ok(ino)
    }

    async fn rmdir(&self, parent: i64, name: &str) -> Result<(), MetaError> {
        self.ensure_writable()?;
        let parent = self.check_root(parent);
        info!("MetaClient: rmdir operation for ({}, '{}')", parent, name);

        self.store.rmdir(parent, name).await?;

        debug!("MetaClient: rmdir completed, updating cache");

        self.inode_cache.remove_child(parent, name).await;
        self.invalidate_parent_path(parent).await;

        Ok(())
    }

    async fn create_file(&self, parent: i64, name: String) -> Result<i64, MetaError> {
        self.ensure_writable()?;
        let parent = self.check_root(parent);
        info!(
            "MetaClient: create_file operation for ({}, '{}')",
            parent, name
        );

        let ino = self.store.create_file(parent, name.clone()).await?;

        info!(
            "MetaClient: create_file created inode {}, updating cache",
            ino
        );

        // Ensure parent node is in cache
        self.inode_cache
            .ensure_node_in_cache(parent, &self.store, None)
            .await?;

        if let Ok(Some(attr)) = self.store.stat(ino).await {
            let cache_parent = (attr.nlink <= 1).then_some(parent);
            self.inode_cache.insert_node(ino, attr, cache_parent).await;
        }
        self.inode_cache.add_child(parent, name, ino).await;

        self.invalidate_parent_path(parent).await;

        Ok(ino)
    }

    async fn link(&self, ino: i64, parent: i64, name: &str) -> Result<FileAttr, MetaError> {
        self.ensure_writable()?;
        let inode = self.check_root(ino);
        let parent = self.check_root(parent);
        info!(
            "MetaClient: link operation for inode {} into ({}, '{}')",
            inode, parent, name
        );

        let attr = self.store.link(inode, parent, name).await?;

        self.inode_cache
            .ensure_node_in_cache(parent, &self.store, None)
            .await?;

        self.inode_cache
            .insert_node(inode, attr.clone(), None)
            .await;
        self.inode_cache
            .add_child(parent, name.to_string(), inode)
            .await;

        self.invalidate_parent_path(parent).await;

        Ok(attr)
    }

    async fn symlink(
        &self,
        parent: i64,
        name: &str,
        target: &str,
    ) -> Result<(i64, FileAttr), MetaError> {
        self.ensure_writable()?;
        let parent = self.check_root(parent);
        info!(
            "MetaClient: symlink operation for ({}, '{}') -> '{}'",
            parent, name, target
        );

        let (ino, attr) = self.store.symlink(parent, name, target).await?;

        debug!("MetaClient: symlink created inode {}, updating cache", ino);

        self.inode_cache
            .ensure_node_in_cache(parent, &self.store, None)
            .await?;

        let cache_parent = (attr.nlink <= 1).then_some(parent);
        self.inode_cache
            .insert_node(ino, attr.clone(), cache_parent)
            .await;
        self.inode_cache
            .add_child(parent, name.to_string(), ino)
            .await;

        self.invalidate_parent_path(parent).await;

        Ok((ino, attr))
    }

    async fn unlink(&self, parent: i64, name: &str) -> Result<(), MetaError> {
        self.ensure_writable()?;
        let parent = self.check_root(parent);
        info!("MetaClient: unlink operation for ({}, '{}')", parent, name);

        self.store.unlink(parent, name).await?;

        debug!("MetaClient: unlink completed, updating cache");

        self.inode_cache.remove_child(parent, name).await;
        self.invalidate_parent_path(parent).await;

        Ok(())
    }

    async fn rename(
        &self,
        old_parent: i64,
        old_name: &str,
        new_parent: i64,
        new_name: String,
    ) -> Result<(), MetaError> {
        self.ensure_writable()?;

        let old_parent = self.check_root(old_parent);
        let new_parent = self.check_root(new_parent);

        // Fast path: if renaming to same location, return success (POSIX no-op)
        if old_parent == new_parent && old_name == new_name {
            return Ok(());
        }

        debug!(
            "MetaClient: rename operation from ({}, '{}') to ({}, '{}')",
            old_parent, old_name, new_parent, new_name
        );

        // Comprehensive pre-validation
        let src_ino = self
            .cached_lookup(old_parent, old_name)
            .await?
            .ok_or_else(|| MetaError::NotFound(old_parent))?;

        let src_attr = self.cached_stat(src_ino).await?;

        // Validate new parent exists and is a directory
        if let Some(parent_attr) = self.cached_stat(new_parent).await? {
            if parent_attr.kind != FileType::Dir {
                return Err(MetaError::NotDirectory(new_parent));
            }
        } else {
            return Err(MetaError::NotFound(new_parent));
        }

        // Validate name constraints
        if new_name.is_empty() || new_name.len() > 255 {
            return Err(MetaError::InvalidFilename);
        }

        if new_name.contains('/') || new_name.contains('\0') {
            return Err(MetaError::InvalidFilename);
        }

        // Execute the store-level rename with atomic cache updates
        self.store
            .rename(old_parent, old_name, new_parent, new_name.clone())
            .await?;

        debug!("MetaClient: rename completed, updating cache");

        // Update cache atomically with enhanced consistency management
        let cache_result = async {
            // Step 1: Store pre-rename cache state for precise invalidation
            let _old_parent_cached = self.inode_cache.get_node(old_parent).await;
            let _new_parent_cached = if old_parent != new_parent {
                self.inode_cache.get_node(new_parent).await
            } else {
                None
            };

            // Step 2: Remove child from old parent (keep inode for later use)
            let child_info = self
                .inode_cache
                .remove_child_but_keep_inode(old_parent, old_name)
                .await;

            if let Some(child_ino) = child_info {
                // Step 3: Ensure new parent is in cache with up-to-date metadata
                self.inode_cache
                    .ensure_node_in_cache(new_parent, &self.store, None)
                    .await?;

                // Step 4: Add child to new parent
                self.inode_cache
                    .add_child(new_parent, new_name.clone(), child_ino)
                    .await;

                // Step 5: Update parent-child relationship based on hard link count
                if let Some(attr) = &src_attr {
                    if attr.nlink <= 1 {
                        // Single link: update parent directly
                        if let Some(child_node) = self.inode_cache.get_node(child_ino).await {
                            child_node.set_parent(new_parent).await;
                        }
                    } else {
                        // Multiple links: clear parent (use LinkParentMeta instead)
                        if let Some(child_node) = self.inode_cache.get_node(child_ino).await {
                            child_node.clear_parent().await;
                        }
                    }
                }
            }

            // Step 6: Precise path cache invalidation
            // Invalidate paths affected by this rename
            self.invalidate_parent_path(old_parent).await;
            if old_parent != new_parent {
                self.invalidate_parent_path(new_parent).await;
            }

            // Step 7: Invalidate directory stat caches (mtime/ctime changed)
            self.inode_cache.invalidate_inode(old_parent).await;
            if old_parent != new_parent {
                self.inode_cache.invalidate_inode(new_parent).await;
            }

            // Step 8: Preload commonly accessed cache entries for better performance
            if let Some(child_ino) = child_info {
                // Preload the renamed entry and its new parent for better subsequent access
                let _ = self.preload_cache_entries(&[child_ino, new_parent]).await;
            }

            Ok::<(), MetaError>(())
        }
        .await;

        if let Err(cache_err) = cache_result {
            warn!(
                "MetaClient: cache update failed after successful store rename: {}",
                cache_err
            );
            // Cache inconsistency is logged but not fatal
        }

        Ok(())
    }

    async fn rename_exchange(
        &self,
        old_parent: i64,
        old_name: &str,
        new_parent: i64,
        new_name: &str,
    ) -> Result<(), MetaError> {
        self.ensure_writable()?;
        let old_parent = self.check_root(old_parent);
        let new_parent = self.check_root(new_parent);

        // Fast path: exchanging with itself is a no-op
        if old_parent == new_parent && old_name == new_name {
            return Ok(());
        }

        debug!(
            "MetaClient: rename_exchange operation between ({}, '{}') and ({}, '{}')",
            old_parent, old_name, new_parent, new_name
        );

        // Both entries must exist
        let old_ino = self
            .cached_lookup(old_parent, old_name)
            .await?
            .ok_or_else(|| MetaError::NotFound(old_parent))?;

        let new_ino = self
            .cached_lookup(new_parent, new_name)
            .await?
            .ok_or_else(|| MetaError::NotFound(new_parent))?;

        // Execute the store-level exchange
        self.store
            .rename_exchange(old_parent, old_name, new_parent, new_name)
            .await?;

        debug!("MetaClient: rename_exchange completed, updating cache");

        // Update cache to reflect the exchange
        let cache_result = async {
            // Invalidate all affected caches
            self.inode_cache.invalidate_inode(old_ino).await;
            self.inode_cache.invalidate_inode(new_ino).await;
            self.inode_cache.invalidate_inode(old_parent).await;
            if old_parent != new_parent {
                self.inode_cache.invalidate_inode(new_parent).await;
            }

            // Invalidate path caches
            self.invalidate_parent_path(old_parent).await;
            if old_parent != new_parent {
                self.invalidate_parent_path(new_parent).await;
            }

            // Update directory entries
            // Remove old entries
            self.inode_cache.remove_child(old_parent, old_name).await;
            self.inode_cache.remove_child(new_parent, new_name).await;

            // Add swapped entries
            self.inode_cache
                .ensure_node_in_cache(old_parent, &self.store, None)
                .await?;
            self.inode_cache
                .ensure_node_in_cache(new_parent, &self.store, None)
                .await?;

            self.inode_cache
                .add_child(old_parent, old_name.to_string(), new_ino)
                .await;
            self.inode_cache
                .add_child(new_parent, new_name.to_string(), old_ino)
                .await;

            Ok::<(), MetaError>(())
        }
        .await;

        if let Err(cache_err) = cache_result {
            warn!(
                "MetaClient: cache update failed after successful rename_exchange: {}",
                cache_err
            );
        }

        Ok(())
    }

    async fn can_rename(
        &self,
        old_parent: i64,
        old_name: &str,
        new_parent: i64,
        new_name: &str,
    ) -> Result<(), MetaError> {
        self.ensure_writable()?;

        // Basic validation
        let src_ino = self
            .cached_lookup(old_parent, old_name)
            .await?
            .ok_or(MetaError::NotFound(old_parent))?;

        let src_attr = self.cached_stat(src_ino).await?;

        // Validate new parent exists and is a directory
        if let Some(parent_attr) = self.cached_stat(new_parent).await? {
            if parent_attr.kind != FileType::Dir {
                return Err(MetaError::NotDirectory(new_parent));
            }
        } else {
            return Err(MetaError::NotFound(new_parent));
        }

        // Validate name constraints
        if new_name.is_empty() || new_name.len() > 255 {
            return Err(MetaError::InvalidFilename);
        }

        if new_name.contains('/') || new_name.contains('\0') {
            return Err(MetaError::InvalidFilename);
        }

        // Check destination constraints
        if let Some(dest_ino) = self.cached_lookup(new_parent, new_name).await? {
            let dest_attr = self
                .cached_stat(dest_ino)
                .await?
                .ok_or(MetaError::NotFound(dest_ino))?;

            match (src_attr.map(|a| a.kind), dest_attr.kind) {
                // Directory replacing directory
                (Some(FileType::Dir), FileType::Dir) => {
                    let children = self.readdir(dest_ino).await?;
                    if !children.is_empty() {
                        return Err(MetaError::DirectoryNotEmpty(dest_ino));
                    }
                }
                // Directory replacing file/symlink - not allowed
                (Some(FileType::Dir), FileType::File)
                | (Some(FileType::Dir), FileType::Symlink) => {
                    return Err(MetaError::NotDirectory(dest_ino));
                }
                // File/symlink replacing directory - not allowed
                (Some(FileType::File), FileType::Dir)
                | (Some(FileType::Symlink), FileType::Dir) => {
                    return Err(MetaError::NotDirectory(dest_ino));
                }
                // File/symlink replacing file/symlink - allowed
                _ => {}
            }
        }

        Ok(())
    }

    async fn rename_with_flags(
        &self,
        old_parent: i64,
        old_name: &str,
        new_parent: i64,
        new_name: String,
        flags: crate::vfs::fs::RenameFlags,
    ) -> Result<(), MetaError> {
        self.ensure_writable()?;

        if flags.exchange {
            // Exchange operation - both must exist
            let _src_ino = self
                .cached_lookup(old_parent, old_name)
                .await?
                .ok_or(MetaError::NotFound(old_parent))?;
            let _dest_ino = self
                .cached_lookup(new_parent, &new_name)
                .await?
                .ok_or(MetaError::NotFound(new_parent))?;

            // Perform exchange (simplified - not truly atomic)
            let temp_name = format!("{}.exchange_temp_{}", old_name, std::process::id());
            self.rename(old_parent, old_name, old_parent, temp_name.clone())
                .await?;
            self.rename(new_parent, &new_name, old_parent, old_name.to_string())
                .await?;
            self.rename(old_parent, &temp_name, new_parent, new_name)
                .await?;
            Ok(())
        } else if flags.noreplace {
            // Check if destination exists
            if self.cached_lookup(new_parent, &new_name).await?.is_some() {
                return Err(MetaError::AlreadyExists {
                    parent: new_parent,
                    name: new_name,
                });
            }
            self.rename(old_parent, old_name, new_parent, new_name)
                .await
        } else {
            // Default behavior
            self.rename(old_parent, old_name, new_parent, new_name)
                .await
        }
    }

    async fn set_file_size(&self, ino: i64, size: u64) -> Result<(), MetaError> {
        self.ensure_writable()?;
        let inode = self.check_root(ino);
        self.store.set_file_size(inode, size).await?;

        // Update cached attribute
        if let Some(node) = self.inode_cache.get_node(inode).await {
            let mut attr = node.attr.write().await;
            attr.size = size;
        }

        Ok(())
    }

    #[tracing::instrument(level = "trace", skip(self), fields(ino, size))]
    async fn extend_file_size(&self, ino: i64, size: u64) -> Result<(), MetaError> {
        self.ensure_writable()?;
        let inode = self.check_root(ino);
        self.store.extend_file_size(inode, size).await?;

        if let Some(node) = self.inode_cache.get_node(inode).await {
            let mut attr = node.attr.write().await;
            if size > attr.size {
                attr.size = size;
            }
        }

        Ok(())
    }

    async fn truncate(&self, ino: i64, size: u64, chunk_size: u64) -> Result<(), MetaError> {
        self.ensure_writable()?;
        let inode = self.check_root(ino);
        self.store.truncate(inode, size, chunk_size).await?;
        self.inode_cache.invalidate_inode(inode).await;
        Ok(())
    }

    async fn get_names(&self, ino: i64) -> Result<Vec<(Option<i64>, String)>, MetaError> {
        let inode = self.check_root(ino);
        if inode == self.root() {
            return Ok(vec![(None, "/".to_string())]);
        }

        self.store.get_names(inode).await
    }

    async fn get_dentries(&self, ino: i64) -> Result<Vec<(i64, String)>, MetaError> {
        let inode = self.check_root(ino);
        if inode == self.root() {
            return Ok(vec![(self.root(), "/".to_string())]);
        }

        self.store.get_dentries(inode).await
    }

    async fn get_dir_parent(&self, dir_ino: i64) -> Result<Option<i64>, MetaError> {
        let inode = self.check_root(dir_ino);
        if inode == self.root() {
            return Ok(None);
        }

        self.store.get_dir_parent(inode).await
    }

    async fn get_paths(&self, ino: i64) -> Result<Vec<String>, MetaError> {
        let inode = self.check_root(ino);
        if inode == self.root() {
            return Ok(vec!["/".to_string()]);
        }

        self.store.get_paths(inode).await
    }

    async fn read_symlink(&self, ino: i64) -> Result<String, MetaError> {
        let inode = self.check_root(ino);
        info!("MetaClient: read_symlink request for inode {}", inode);
        self.store.read_symlink(inode).await
    }

    async fn set_attr(
        &self,
        ino: i64,
        req: &SetAttrRequest,
        flags: SetAttrFlags,
    ) -> Result<FileAttr, MetaError> {
        self.ensure_writable()?;
        let inode = self.check_root(ino);
        let attr = self.store.set_attr(inode, req, flags).await?;
        self.inode_cache
            .insert_node(inode, attr.clone(), None)
            .await;
        Ok(attr)
    }

    async fn open(&self, ino: i64, flags: OpenFlags) -> Result<FileAttr, MetaError> {
        let inode = self.check_root(ino);
        self.store.open(inode, flags).await
    }

    async fn close(&self, ino: i64) -> Result<(), MetaError> {
        let inode = self.check_root(ino);
        self.store.close(inode).await
    }

    async fn get_deleted_files(&self) -> Result<Vec<i64>, MetaError> {
        self.store.get_deleted_files().await
    }

    async fn remove_file_metadata(&self, ino: i64) -> Result<(), MetaError> {
        self.ensure_writable()?;
        self.store.remove_file_metadata(ino).await
    }

    async fn get_slices(&self, chunk_id: u64) -> Result<Vec<SliceDesc>, MetaError> {
        let (inode, chunk_index) = extract_ino_and_chunk_index(chunk_id);
        if let Some(slices) = self.inode_cache.get_slices(inode, chunk_index).await {
            return Ok(slices);
        }
        self.store.get_slices(chunk_id).await
    }

    async fn append_slice(&self, chunk_id: u64, slice: SliceDesc) -> Result<(), MetaError> {
        self.ensure_writable()?;

        let (inode, chunk_index) = extract_ino_and_chunk_index(chunk_id);
        self.store.append_slice(chunk_id, slice).await?;
        self.inode_cache
            .append_slice(inode, chunk_index, slice)
            .await;
        Ok(())
    }

    async fn next_id(&self, key: &str) -> Result<i64, MetaError> {
        self.ensure_writable()?;
        self.store.next_id(key).await
    }

    async fn start_session(&self, session_info: SessionInfo) -> Result<(), MetaError> {
        MetaClient::start_session(self, session_info).await
    }

    async fn shutdown_session(&self) -> Result<(), MetaError> {
        MetaClient::shutdown_session(self).await;
        Ok(())
    }

    async fn get_plock(
        &self,
        inode: i64,
        query: &FileLockQuery,
    ) -> Result<FileLockInfo, MetaError> {
        self.store.get_plock(inode, query).await
    }

    async fn set_plock(
        &self,
        inode: i64,
        owner: i64,
        block: bool,
        lock_type: FileLockType,
        range: FileLockRange,
        pid: u32,
    ) -> Result<(), MetaError> {
        self.store
            .set_plock(inode, owner, block, lock_type, range, pid)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meta::config::{CacheConfig, ClientOptions, Config, DatabaseConfig, DatabaseType};
    use crate::meta::stores::database_store::DatabaseMetaStore;
    use crate::vfs::chunk_id_for;
    use std::time::Duration;

    async fn create_test_client() -> Arc<MetaClient<DatabaseMetaStore>> {
        create_test_client_with_capacity(100, 100).await
    }

    async fn create_test_client_with_capacity(
        inode_capacity: usize,
        path_capacity: usize,
    ) -> Arc<MetaClient<DatabaseMetaStore>> {
        let db_path = "sqlite::memory:".to_string();

        let config = Config {
            database: DatabaseConfig {
                db_config: DatabaseType::Sqlite { url: db_path },
            },
            cache: CacheConfig::default(),
            client: ClientOptions::default(),
        };

        let store = Arc::new(DatabaseMetaStore::from_config(config).await.unwrap());

        let capacity = CacheCapacity {
            inode: inode_capacity,
            path: path_capacity,
        };

        let ttl = CacheTtl {
            inode_ttl: Duration::from_secs(60),
            path_ttl: Duration::from_secs(60),
        };

        MetaClient::new(store, capacity, ttl)
    }

    #[tokio::test]
    async fn test_rename_operations() {
        let client = create_test_client().await;

        // Create test structure
        let dir1 = client.mkdir(1, "dir1".to_string()).await.unwrap();
        let dir2 = client.mkdir(1, "dir2".to_string()).await.unwrap();
        let file1 = client
            .create_file(dir1, "old_name.txt".to_string())
            .await
            .unwrap();

        // Scenario 1: Rename within same directory
        client
            .rename(dir1, "old_name.txt", dir1, "new_name.txt".to_string())
            .await
            .unwrap();

        // Verify old name doesn't exist
        let old_lookup = client.lookup(dir1, "old_name.txt").await.unwrap();
        assert_eq!(old_lookup, None, "Old name should not exist");

        // Verify new name exists
        let new_lookup = client.lookup(dir1, "new_name.txt").await.unwrap();
        assert_eq!(
            new_lookup,
            Some(file1),
            "New name should point to same inode"
        );

        // readdir should show new name
        let entries = client.readdir(dir1).await.unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "new_name.txt");

        // Scenario 2: Move across directories
        client
            .rename(dir1, "new_name.txt", dir2, "moved_file.txt".to_string())
            .await
            .unwrap();

        // dir1 should be empty
        let dir1_entries = client.readdir(dir1).await.unwrap();
        assert_eq!(dir1_entries.len(), 0, "dir1 should be empty after move");

        // dir2 should contain moved file
        let dir2_entries = client.readdir(dir2).await.unwrap();
        assert_eq!(dir2_entries.len(), 1, "dir2 should have 1 file");
        assert_eq!(dir2_entries[0].name, "moved_file.txt");

        // Verify path resolution
        let resolved = client.resolve_path("/dir2/moved_file.txt").await.unwrap();
        assert_eq!(resolved, file1, "Path should resolve to correct inode");

        let path = client
            .get_paths(file1)
            .await
            .unwrap()
            .first()
            .cloned()
            .unwrap();
        assert_eq!(
            path, "/dir2/moved_file.txt",
            "get_path should return new path"
        );
    }

    #[tokio::test]
    async fn test_slice_operations() {
        let client = create_test_client().await;

        let ino = client.create_file(1, "text".to_string()).await.unwrap();
        let chunk_id = chunk_id_for(ino, 1);

        let test_slices = (1..=10)
            .map(|e| crate::chuck::SliceDesc {
                slice_id: e,
                chunk_id,
                offset: 0,
                length: 100,
            })
            .collect::<Vec<_>>();

        for desc in test_slices.iter().copied() {
            client.append_slice(chunk_id, desc).await.unwrap();
        }

        let from_method = client.get_slices(chunk_id).await.unwrap();
        assert_eq!(test_slices, from_method);

        let (ino, chunk_index) = extract_ino_and_chunk_index(chunk_id);
        let from_cached = client
            .inode_cache
            .get_slices(ino, chunk_index)
            .await
            .unwrap();
        assert_eq!(test_slices, from_cached);
    }

    /// Test scenario: Complex sequence of mixed operations
    ///
    #[tokio::test]
    async fn test_complex_mixed_operations() {
        let client = create_test_client().await;

        // Create initial structure
        let dir1 = client.mkdir(1, "dir1".to_string()).await.unwrap();
        let file1 = client
            .create_file(dir1, "file1.txt".to_string())
            .await
            .unwrap();
        let file2 = client
            .create_file(dir1, "file2.txt".to_string())
            .await
            .unwrap();

        // Verify initial state
        let dir1_entries = client.readdir(dir1).await.unwrap();
        assert_eq!(dir1_entries.len(), 2);

        // Rename file1 within the same directory
        client
            .rename(dir1, "file1.txt", dir1, "renamed1.txt".to_string())
            .await
            .unwrap();

        // Verify rename worked
        let dir1_entries = client.readdir(dir1).await.unwrap();
        assert_eq!(dir1_entries.len(), 2);
        let names: std::collections::HashSet<String> =
            dir1_entries.iter().map(|e| e.name.clone()).collect();
        assert!(names.contains("renamed1.txt"));
        assert!(names.contains("file2.txt"));
        assert!(!names.contains("file1.txt"));

        // Create a subdirectory
        let subdir = client.mkdir(dir1, "subdir".to_string()).await.unwrap();

        // Move file2 into subdirectory
        client
            .rename(dir1, "file2.txt", subdir, "moved2.txt".to_string())
            .await
            .unwrap();

        // Verify move worked
        let dir1_entries = client.readdir(dir1).await.unwrap();
        assert_eq!(dir1_entries.len(), 2); // renamed1.txt + subdir
        let subdir_entries = client.readdir(subdir).await.unwrap();
        assert_eq!(subdir_entries.len(), 1);
        assert_eq!(subdir_entries[0].name, "moved2.txt");

        // Verify path resolution still works
        let path1 = client
            .get_paths(file1)
            .await
            .unwrap()
            .first()
            .cloned()
            .unwrap();
        assert_eq!(path1, "/dir1/renamed1.txt");

        let path2 = client
            .get_paths(file2)
            .await
            .unwrap()
            .first()
            .cloned()
            .unwrap();
        assert_eq!(path2, "/dir1/subdir/moved2.txt");

        // Create hard link and test rename behavior
        let _link_attr = client.link(file1, 1, "link1.txt").await.unwrap();

        // Verify both paths exist
        let paths = client.get_paths(file1).await.unwrap();
        assert_eq!(paths.len(), 2);
        let path_set: std::collections::HashSet<String> = paths.into_iter().collect();
        assert!(path_set.contains("/dir1/renamed1.txt"));
        assert!(path_set.contains("/link1.txt"));

        // Rename one of the links
        client
            .rename(1, "link1.txt", 1, "renamed_link.txt".to_string())
            .await
            .unwrap();

        // Verify paths updated correctly
        let paths = client.get_paths(file1).await.unwrap();
        assert_eq!(paths.len(), 2);
        let path_set: std::collections::HashSet<String> = paths.into_iter().collect();
        assert!(path_set.contains("/dir1/renamed1.txt"));
        assert!(path_set.contains("/renamed_link.txt"));
        assert!(!path_set.contains("/link1.txt"));
    }

    #[test]
    fn test_normalize_path() {
        // Basic absolute paths
        assert_eq!(MetaClient::<DatabaseMetaStore>::normalize_path("/"), "/");
        assert_eq!(
            MetaClient::<DatabaseMetaStore>::normalize_path("/home/user"),
            "/home/user"
        );

        // Handle .
        assert_eq!(MetaClient::<DatabaseMetaStore>::normalize_path("/./"), "/");
        assert_eq!(
            MetaClient::<DatabaseMetaStore>::normalize_path("/home/./user"),
            "/home/user"
        );

        // Handle ..
        assert_eq!(
            MetaClient::<DatabaseMetaStore>::normalize_path("/home/user/../"),
            "/home"
        );
        assert_eq!(
            MetaClient::<DatabaseMetaStore>::normalize_path("/home/../user"),
            "/user"
        );

        // Complex cases
        assert_eq!(
            MetaClient::<DatabaseMetaStore>::normalize_path("/a/b/../c/./d"),
            "/a/c/d"
        );
        assert_eq!(
            MetaClient::<DatabaseMetaStore>::normalize_path("/a/./b/../../c"),
            "/c"
        );

        // Relative paths
        assert_eq!(
            MetaClient::<DatabaseMetaStore>::normalize_path("file.txt"),
            "file.txt"
        );
        assert_eq!(
            MetaClient::<DatabaseMetaStore>::normalize_path("../file.txt"),
            "file.txt"
        );

        // Edge cases
        assert_eq!(MetaClient::<DatabaseMetaStore>::normalize_path(""), "");
        assert_eq!(MetaClient::<DatabaseMetaStore>::normalize_path("."), "");
        assert_eq!(
            MetaClient::<DatabaseMetaStore>::normalize_path("/../../../file.txt"),
            "/file.txt"
        );
    }

    /// Test scenario: Delete operations and cache invalidation
    ///
    /// Verify delete operations work correctly and cache is properly invalidated
    #[tokio::test]
    async fn test_delete_operations() {
        let client = create_test_client().await;

        let dir1 = client.mkdir(1, "dir1".to_string()).await.unwrap();

        // Create files
        let _f1 = client
            .create_file(dir1, "f1.txt".to_string())
            .await
            .unwrap();
        let _f2 = client
            .create_file(dir1, "f2.txt".to_string())
            .await
            .unwrap();
        let _f3 = client
            .create_file(dir1, "f3.txt".to_string())
            .await
            .unwrap();
        let _f4 = client
            .create_file(dir1, "f4.txt".to_string())
            .await
            .unwrap();

        // readdir should include new file
        let entries2 = client.readdir(dir1).await.unwrap();
        assert_eq!(entries2.len(), 4, "Should see all 4 files");

        // Delete a file
        client.unlink(dir1, "f2.txt").await.unwrap();

        // readdir should reflect deletion
        let entries3 = client.readdir(dir1).await.unwrap();
        assert_eq!(entries3.len(), 3, "Should have 3 files after deletion");

        let names3: Vec<String> = entries3.iter().map(|e| e.name.clone()).collect();
        assert!(
            !names3.contains(&"f2.txt".to_string()),
            "Deleted file should not appear"
        );
    }

    /// Test scenario: Empty directory handling
    ///
    /// Verify readdir behavior and cache handling for empty directories
    #[tokio::test]
    async fn test_empty_directory_handling() {
        let client = create_test_client().await;

        // Create empty directory
        let empty_dir = client.mkdir(1, "empty".to_string()).await.unwrap();

        // readdir on empty directory
        let entries = client.readdir(empty_dir).await.unwrap();
        assert_eq!(entries.len(), 0, "Empty directory should have no entries");

        // readdir again (should hit cache)
        let entries2 = client.readdir(empty_dir).await.unwrap();
        assert_eq!(entries2.len(), 0, "Cached result should also be empty");

        // Add file
        let file = client
            .create_file(empty_dir, "first.txt".to_string())
            .await
            .unwrap();

        // readdir should show new file
        let entries3 = client.readdir(empty_dir).await.unwrap();
        assert_eq!(entries3.len(), 1, "Should have 1 file");
        assert_eq!(entries3[0].name, "first.txt");
        assert_eq!(entries3[0].ino, file);

        // Delete file, restore to empty
        client.unlink(empty_dir, "first.txt").await.unwrap();

        // Should be empty again
        let entries4 = client.readdir(empty_dir).await.unwrap();
        assert_eq!(entries4.len(), 0, "Should be empty again");
    }

    #[tokio::test]
    async fn test_get_parent_and_name() {
        let client = create_test_client().await;

        let dir1 = client.mkdir(1, "parent_dir".to_string()).await.unwrap();
        let file1 = client
            .create_file(dir1, "child_file.txt".to_string())
            .await
            .unwrap();

        let root_parent = client.get_dir_parent(dir1).await.unwrap().unwrap();
        assert_eq!(root_parent, 1, "Parent of dir1 should be root");

        let file_links = client.get_dentries(file1).await.unwrap();
        assert!(
            file_links.contains(&(dir1, "child_file.txt".to_string())),
            "File should have expected (parent,name) link"
        );

        let dir_links = client.get_names(dir1).await.unwrap();
        assert!(
            dir_links.contains(&(Some(1), "parent_dir".to_string())),
            "Directory should have expected (parent,name) link"
        );

        let root_links = client.get_names(1).await.unwrap();
        assert_eq!(root_links, vec![(None, "/".to_string())]);
    }

    #[tokio::test]
    async fn test_hardlink_get_names_and_rename_one_link() {
        let client = create_test_client().await;

        let links = client.mkdir(1, "links".to_string()).await.unwrap();
        let file_ino = client
            .create_file(links, "a.txt".to_string())
            .await
            .unwrap();

        client.link(file_ino, links, "b.txt").await.unwrap();

        let names = client.get_names(file_ino).await.unwrap();
        assert!(names.contains(&(Some(links), "a.txt".to_string())));
        assert!(names.contains(&(Some(links), "b.txt".to_string())));

        client
            .rename(links, "b.txt", links, "c.txt".to_string())
            .await
            .unwrap();

        let names = client.get_names(file_ino).await.unwrap();
        assert!(names.contains(&(Some(links), "a.txt".to_string())));
        assert!(names.contains(&(Some(links), "c.txt".to_string())));
        assert!(!names.contains(&(Some(links), "b.txt".to_string())));

        client.unlink(links, "c.txt").await.unwrap();

        let names = client.get_names(file_ino).await.unwrap();
        assert_eq!(names, vec![(Some(links), "a.txt".to_string())]);
    }

    #[tokio::test]
    async fn test_hardlink_link_should_not_poison_cached_parent() {
        let client = create_test_client().await;

        let d1 = client.mkdir(1, "d1".to_string()).await.unwrap();
        let d2 = client.mkdir(1, "d2".to_string()).await.unwrap();

        let file_ino = client.create_file(d1, "a.txt".to_string()).await.unwrap();

        client.link(file_ino, d2, "b.txt").await.unwrap();

        let dentries = client.get_dentries(file_ino).await.unwrap();
        assert!(dentries.contains(&(d1, "a.txt".to_string())));
        assert!(dentries.contains(&(d2, "b.txt".to_string())));
    }

    /// Test scenario: Intelligent path cache invalidation
    ///
    /// Verify path cache invalidation strategy:
    /// 1. Modifying a directory only invalidates related paths
    /// 2. Unrelated paths should remain cached
    #[tokio::test]
    async fn test_intelligent_path_invalidation() {
        let client = create_test_client().await;

        // Create directory structure:
        // /dira/
        // /dira/file1.txt
        // /dirb/
        // /dirb/file2.txt
        let dira = client.mkdir(1, "dira".to_string()).await.unwrap();
        let dirb = client.mkdir(1, "dirb".to_string()).await.unwrap();

        let _file1 = client
            .create_file(dira, "file1.txt".to_string())
            .await
            .unwrap();
        let _file2 = client
            .create_file(dirb, "file2.txt".to_string())
            .await
            .unwrap();

        // Resolve all paths to populate cache
        let _ino_dira = client.resolve_path("/dira").await.unwrap();
        let _ino_file1 = client.resolve_path("/dira/file1.txt").await.unwrap();
        let _ino_dirb = client.resolve_path("/dirb").await.unwrap();
        let _ino_file2 = client.resolve_path("/dirb/file2.txt").await.unwrap();

        // Verify all paths are cached
        assert!(client.path_cache.get("/dira").await.is_some());
        assert!(client.path_cache.get("/dira/file1.txt").await.is_some());
        assert!(client.path_cache.get("/dirb").await.is_some());
        assert!(client.path_cache.get("/dirb/file2.txt").await.is_some());

        // Create new file in /dira (triggers invalidation)
        let _file3 = client
            .create_file(dira, "file3.txt".to_string())
            .await
            .unwrap();

        // Verify intelligent invalidation:
        // - /dira and its sub-paths should be invalidated
        // - /dirb and its sub-paths should remain cached (unrelated)

        // Re-resolve /dirb paths - should hit cache
        let ino_dirb_after = client.resolve_path("/dirb").await.unwrap();
        assert_eq!(ino_dirb_after, dirb, "/dirb should still be cached");

        let ino_file2_after = client.resolve_path("/dirb/file2.txt").await.unwrap();
        assert_eq!(
            ino_file2_after, _file2,
            "/dirb/file2.txt should still be cached"
        );

        // Verify new file is accessible
        let ino_file3 = client.resolve_path("/dira/file3.txt").await.unwrap();
        assert_eq!(ino_file3, _file3);
    }

    #[tokio::test]
    async fn test_rename_same_location_noop() {
        use std::sync::atomic::Ordering;

        let client = create_test_client().await;

        // Create test file
        let root = client.root.load(Ordering::Relaxed);
        let file_ino = client
            .create_file(root, "test.txt".to_string())
            .await
            .unwrap();

        // Get original attributes
        let original_attr = client.cached_stat(file_ino).await.unwrap().unwrap();

        // Test 1: Rename to same location should succeed as no-op
        client
            .rename(root, "test.txt", root, "test.txt".to_string())
            .await
            .unwrap();

        // Verify file still exists and unchanged
        let after_attr = client.cached_stat(file_ino).await.unwrap().unwrap();
        assert_eq!(original_attr.ino, after_attr.ino);

        // Test 2: Verify lookup still works
        let looked_up = client.lookup(root, "test.txt").await.unwrap().unwrap();
        assert_eq!(looked_up, file_ino);

        // Test 3: rename_exchange with same location should also succeed
        client
            .rename_exchange(root, "test.txt", root, "test.txt")
            .await
            .unwrap();

        // Verify still exists
        let final_attr = client.cached_stat(file_ino).await.unwrap().unwrap();
        assert_eq!(original_attr.ino, final_attr.ino);
    }
}
