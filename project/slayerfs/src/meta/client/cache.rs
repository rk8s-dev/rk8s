use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use crate::chunk::SliceDesc;
use crate::meta::entities::etcd::EtcdEntryInfo;
use crate::meta::store::{DirEntry, FileAttr, MetaError, MetaStore};
use crate::vfs::fs::FileType;
use dashmap::{DashMap, Entry};
use moka::future::Cache;
use moka::notification::RemovalCause;
use tokio::sync::RwLock;
use tracing::info;

/// Type alias for children map to reduce complexity
/// BTreeMap provides ordered iteration for consistent ls output
pub(crate) type ChildrenMap = BTreeMap<String, i64>;

#[derive(Clone)]
pub(crate) enum ChildrenState {
    /// Children not yet loaded from database
    NotLoaded,
    /// Partially loaded - only entries added via incremental operations (mkdir/create_file)
    /// Should reload from DB on next readdir to ensure completeness
    Partial(Arc<ChildrenMap>),
    /// Fully loaded from database - safe to return in readdir without reloading
    Complete(Arc<ChildrenMap>),
}

impl ChildrenState {
    /// Check if children are fully loaded
    pub(crate) fn is_complete(&self) -> bool {
        matches!(self, ChildrenState::Complete(_))
    }

    /// Get the underlying map if available (both Partial and Complete)
    pub(crate) fn get_map(&self) -> Option<&Arc<ChildrenMap>> {
        match self {
            ChildrenState::Partial(map) | ChildrenState::Complete(map) => Some(map),
            ChildrenState::NotLoaded => None,
        }
    }
}

/// Inode metadata entry in cache
///
/// Represents a cached inode with mutable fields that can be updated in-place.
/// Uses RwLock for async-safe concurrent access to mutable data.
pub(crate) struct InodeEntry {
    /// File attributes (size, permissions, timestamps, etc.) of THIS inode
    pub(crate) attr: Arc<RwLock<FileAttr>>,
    /// Parent directory's inode number (None for root directory)
    pub(crate) parent: Arc<RwLock<Option<i64>>>,
    /// Directory children with loading state tracking (only used if THIS inode is a directory)
    pub(crate) children: Arc<RwLock<ChildrenState>>,
    /// Monotonic generation for child map mutations to detect stale readdir snapshots.
    pub(crate) children_generation: Arc<AtomicU64>,
    /// Cache slice metadata per chunk index
    pub(crate) slices: DashMap<u64, Vec<SliceDesc>>,
}

impl Clone for InodeEntry {
    fn clone(&self) -> Self {
        Self {
            attr: Arc::clone(&self.attr),
            parent: Arc::clone(&self.parent),
            children: Arc::clone(&self.children),
            children_generation: Arc::clone(&self.children_generation),
            slices: self.slices.clone(),
        }
    }
}

impl InodeEntry {
    pub(crate) fn new(attr: FileAttr, parent: Option<i64>) -> Self {
        Self {
            attr: Arc::new(RwLock::new(attr)),
            parent: Arc::new(RwLock::new(parent)),
            children: Arc::new(RwLock::new(ChildrenState::NotLoaded)),
            children_generation: Arc::new(AtomicU64::new(0)),
            slices: DashMap::new(),
        }
    }

    pub(crate) async fn set_parent(&self, parent_ino: i64) {
        *self.parent.write().await = Some(parent_ino);
    }

    pub(crate) async fn clear_parent(&self) {
        *self.parent.write().await = None;
    }
}

/// Dual-layer inode cache: Moka for TTL/LRU + DashMap for concurrent mutations
///
/// Architecture:
/// - `entries`: DashMap for lock-free concurrent read/write access to cached data
/// - `ttl_manager`: Moka cache for automatic expiration and capacity management
pub(crate) struct InodeCache {
    entries: Arc<DashMap<i64, Arc<InodeEntry>>>,
    ttl_manager: Cache<i64, Arc<InodeEntry>>,
}

impl InodeCache {
    /// Creates a new InodeCache with specified capacity and TTL.
    pub(crate) fn new(capacity: u64, ttl: Duration) -> Self {
        let entries = Arc::new(DashMap::new());
        let entries_clone = entries.clone();

        let ttl_manager = Cache::builder()
            .max_capacity(capacity)
            .time_to_idle(ttl)
            .eviction_listener(
                move |key: Arc<i64>, _value: Arc<InodeEntry>, cause: RemovalCause| {
                    info!("InodeCache: Evicting inode {} (cause: {:?})", key, cause);
                    entries_clone.remove(&*key);
                },
            )
            .build();

        Self {
            entries,
            ttl_manager,
        }
    }

    pub(crate) async fn insert_node(&self, ino: i64, attr: FileAttr, parent: Option<i64>) {
        match self.ttl_manager.get(&ino).await {
            Some(node) => {
                *node.attr.write().await = attr;
                *node.parent.write().await = parent;
            }
            None => {
                let node = Arc::new(InodeEntry::new(attr, parent));
                self.entries.insert(ino, node.clone());
                self.ttl_manager.insert(ino, node).await;
            }
        }
    }

    pub(crate) async fn invalidate_inode(&self, ino: i64) {
        self.ttl_manager.invalidate(&ino).await;
        self.entries.remove(&ino);
    }

    pub(crate) async fn ensure_node_in_cache(
        &self,
        ino: i64,
        store: &dyn MetaStore,
        parent: Option<i64>,
    ) -> Result<bool, MetaError> {
        if self.get_node(ino).await.is_some() {
            return Ok(true);
        }

        if let Some(attr) = store.stat(ino).await? {
            self.insert_node(ino, attr, parent).await;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub(crate) async fn add_child(&self, parent_ino: i64, name: String, child_ino: i64) {
        if let Some(parent_node) = self.ttl_manager.get(&parent_ino).await {
            let mut children_lock = parent_node.children.write().await;

            let new_map = match &*children_lock {
                ChildrenState::NotLoaded => {
                    let mut map = BTreeMap::new();
                    map.insert(name, child_ino);
                    ChildrenState::Partial(Arc::new(map))
                }
                ChildrenState::Partial(existing_map) | ChildrenState::Complete(existing_map) => {
                    let mut map = (**existing_map).clone();
                    map.insert(name, child_ino);
                    match &*children_lock {
                        ChildrenState::Complete(_) => ChildrenState::Complete(Arc::new(map)),
                        _ => ChildrenState::Partial(Arc::new(map)),
                    }
                }
            };

            *children_lock = new_map;
            parent_node
                .children_generation
                .fetch_add(1, Ordering::AcqRel);
        }
    }

    pub(crate) async fn children_generation(&self, parent_ino: i64) -> Option<u64> {
        let parent_node = self.ttl_manager.get(&parent_ino).await?;
        Some(parent_node.children_generation.load(Ordering::Acquire))
    }

    pub(crate) async fn load_children_if_fresh(
        &self,
        parent_ino: i64,
        entries: Vec<(String, i64)>,
        expected_generation: u64,
    ) -> bool {
        if let Some(parent_node) = self.ttl_manager.get(&parent_ino).await {
            let mut children_lock = parent_node.children.write().await;
            if parent_node.children_generation.load(Ordering::Acquire) != expected_generation {
                return false;
            }
            let new_children = Arc::new(entries.into_iter().collect::<BTreeMap<_, _>>());
            *children_lock = ChildrenState::Complete(new_children);
            return true;
        }

        false
    }

    pub(crate) async fn remove_child(&self, parent_ino: i64, name: &str) -> Option<i64> {
        if let Some(parent_node) = self.ttl_manager.get(&parent_ino).await {
            let mut children_lock = parent_node.children.write().await;
            if let Some(children_map) = children_lock.get_map() {
                let mut map = (**children_map).clone();
                if let Some(child_ino) = map.remove(name) {
                    *children_lock = match &*children_lock {
                        ChildrenState::Complete(_) => ChildrenState::Complete(Arc::new(map)),
                        _ => ChildrenState::Partial(Arc::new(map)),
                    };
                    parent_node
                        .children_generation
                        .fetch_add(1, Ordering::AcqRel);

                    self.ttl_manager.invalidate(&child_ino).await;
                    return Some(child_ino);
                }
            }
        }
        None
    }

    pub(crate) async fn remove_child_but_keep_inode(
        &self,
        parent_ino: i64,
        name: &str,
    ) -> Option<i64> {
        if let Some(parent_node) = self.ttl_manager.get(&parent_ino).await {
            let mut children_lock = parent_node.children.write().await;
            if let Some(children_map) = children_lock.get_map() {
                let mut map = (**children_map).clone();
                if let Some(child_ino) = map.remove(name) {
                    *children_lock = match &*children_lock {
                        ChildrenState::Complete(_) => ChildrenState::Complete(Arc::new(map)),
                        _ => ChildrenState::Partial(Arc::new(map)),
                    };
                    parent_node
                        .children_generation
                        .fetch_add(1, Ordering::AcqRel);
                    return Some(child_ino);
                }
            }
        }
        None
    }

    pub(crate) async fn lookup(&self, parent_ino: i64, name: &str) -> Option<i64> {
        let parent_node = self.ttl_manager.get(&parent_ino).await?;
        let children_lock = parent_node.children.read().await;
        let children_map = children_lock.get_map()?;
        children_map.get(name).copied()
    }

    pub(crate) async fn readdir(&self, ino: i64) -> Option<Vec<DirEntry>> {
        let node = self.ttl_manager.get(&ino).await?;
        let children_lock = node.children.read().await;

        if !children_lock.is_complete() {
            return None;
        }

        let children_map = children_lock.get_map()?;
        let mut entries = Vec::new();

        for (name, child_ino) in children_map.iter() {
            let kind = if let Some(child) = self.ttl_manager.get(child_ino).await {
                child.attr.read().await.kind
            } else {
                FileType::File
            };

            entries.push(DirEntry {
                ino: *child_ino,
                name: name.clone(),
                kind,
            });
        }

        Some(entries)
    }

    pub(crate) async fn get_attr(&self, ino: i64) -> Option<FileAttr> {
        let node = self.ttl_manager.get(&ino).await?;
        Some(node.attr.read().await.clone())
    }

    pub(crate) async fn get_node(&self, ino: i64) -> Option<Arc<InodeEntry>> {
        self.ttl_manager.get(&ino).await
    }

    pub(crate) async fn update_metadata(&self, ino: i64, metadata: EtcdEntryInfo) {
        if let Some(node) = self.ttl_manager.get(&ino).await {
            let new_attr = metadata.to_file_attr(ino);
            *node.attr.write().await = new_attr;
        }
    }

    pub(crate) async fn append_slice(&self, inode: i64, chunk_index: u64, desc: SliceDesc) {
        if let Some(node) = self.ttl_manager.get(&inode).await {
            // Only append to an EXISTING cached slice list that was populated
            // from a full backend read (via get_slices → cache_slices_if_absent).
            // Do NOT create a new entry from a single slice — that produces a
            // partial list that get_slices would trust as authoritative, causing
            // it to miss slices that exist in the backing store.  This matters
            // after truncate: invalidate_inode clears the cache, insert_node
            // recreates it with an empty slices map, and post-truncate writes
            // would otherwise seed a partial entry that shadows the full list.
            if let Some(mut slices) = node.slices.get_mut(&chunk_index) {
                slices.push(desc);
            }
        }
    }

    #[allow(dead_code)]
    pub(crate) async fn replace_slices(&self, inode: i64, chunk_index: u64, desc: &[SliceDesc]) {
        if let Some(node) = self.ttl_manager.get(&inode).await {
            // Same rationale as append_slice: only update an existing entry.
            if let Some(mut slices) = node.slices.get_mut(&chunk_index) {
                *slices = desc.to_owned();
            }
        }
    }

    pub(crate) async fn cache_slices_if_absent(
        &self,
        inode: i64,
        chunk_index: u64,
        desc: &[SliceDesc],
    ) -> Option<Vec<SliceDesc>> {
        let node = self.ttl_manager.get(&inode).await?;
        let entry = node.slices.entry(chunk_index);
        Some(match entry {
            Entry::Occupied(entry) => entry.get().clone(),
            Entry::Vacant(entry) => {
                let cached = desc.to_owned();
                entry.insert(cached.clone());
                cached
            }
        })
    }

    pub(crate) async fn invalidate_slices(&self, inode: i64, chunk_index: u64) {
        if let Some(node) = self.ttl_manager.get(&inode).await {
            node.slices.remove(&chunk_index);
        }
    }

    pub(crate) async fn get_slices(&self, inode: i64, chunk_index: u64) -> Option<Vec<SliceDesc>> {
        let node = self.ttl_manager.get(&inode).await?;

        // Use entry api to ensure consistency, it will lock the bucket.
        let entry = node.slices.entry(chunk_index);
        match entry {
            Entry::Occupied(entry) => entry.get().clone().into(),
            Entry::Vacant(_) => None,
        }
    }

    pub(crate) async fn replace_children(&self, parent_ino: i64, children: HashMap<String, i64>) {
        if let Some(parent_node) = self.ttl_manager.get(&parent_ino).await {
            let mut children_lock = parent_node.children.write().await;
            let new_children = Arc::new(children.into_iter().collect::<BTreeMap<_, _>>());
            *children_lock = ChildrenState::Complete(new_children);
            parent_node
                .children_generation
                .fetch_add(1, Ordering::AcqRel);
        }
    }
}
