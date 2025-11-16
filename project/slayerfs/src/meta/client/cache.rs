use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::Duration;

use dashmap::DashMap;
use moka::future::Cache;
use moka::notification::RemovalCause;
use tokio::sync::RwLock;
use tracing::info;

use crate::meta::entities::etcd::EtcdEntryInfo;
use crate::meta::store::{DirEntry, FileAttr, MetaError, MetaStore};
use crate::vfs::fs::FileType;

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
#[derive(Clone)]
pub(crate) struct InodeEntry {
    /// File attributes (size, permissions, timestamps, etc.) of THIS inode
    pub(crate) attr: Arc<RwLock<FileAttr>>,
    /// Parent directory's inode number (None for root directory)
    pub(crate) parent: Arc<RwLock<Option<i64>>>,
    /// Directory children with loading state tracking (only used if THIS inode is a directory)
    pub(crate) children: Arc<RwLock<ChildrenState>>,
}

impl InodeEntry {
    pub(crate) fn new(attr: FileAttr, parent: Option<i64>) -> Self {
        Self {
            attr: Arc::new(RwLock::new(attr)),
            parent: Arc::new(RwLock::new(parent)),
            children: Arc::new(RwLock::new(ChildrenState::NotLoaded)),
        }
    }

    pub(crate) async fn get_parent(&self) -> Option<i64> {
        *self.parent.read().await
    }

    pub(crate) async fn set_parent(&self, parent_ino: i64) {
        *self.parent.write().await = Some(parent_ino);
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
            .time_to_live(ttl)
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
        if let Some(existing_node) = self.ttl_manager.get(&ino).await {
            *existing_node.attr.write().await = attr;
            if let Some(p) = parent {
                existing_node.set_parent(p).await;
            }
        } else {
            let node = Arc::new(InodeEntry::new(attr, parent));
            self.entries.insert(ino, node.clone());
            self.ttl_manager.insert(ino, node).await;
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
        }
    }

    pub(crate) async fn load_children(&self, parent_ino: i64, entries: Vec<(String, i64)>) {
        if let Some(parent_node) = self.ttl_manager.get(&parent_ino).await {
            let mut children_lock = parent_node.children.write().await;
            let new_children = Arc::new(entries.into_iter().collect::<BTreeMap<_, _>>());
            *children_lock = ChildrenState::Complete(new_children);
        }
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

    pub(crate) async fn replace_children(&self, parent_ino: i64, children: HashMap<String, i64>) {
        if let Some(parent_node) = self.ttl_manager.get(&parent_ino).await {
            let mut children_lock = parent_node.children.write().await;
            let new_children = Arc::new(children.into_iter().collect::<BTreeMap<_, _>>());
            *children_lock = ChildrenState::Complete(new_children);
        }
    }
}
