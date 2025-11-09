use crate::meta::config::{CacheCapacity, CacheTtl};
use crate::meta::store::{DirEntry, FileAttr, MetaError, MetaStore};
use crate::vfs::fs::FileType;
use async_recursion::async_recursion;
use async_trait::async_trait;
use dashmap::DashMap;
use moka::future::Cache;
use moka::notification::RemovalCause;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::info;

/// Type alias for children map to reduce complexity
type ChildrenMap = DashMap<String, i64>;

/// Trie node for efficient path prefix matching
///
/// Each node represents a path component (directory or file name).
/// The tree structure allows O(depth) prefix-based invalidation.
struct TrieNode {
    /// Child nodes keyed by path component name
    children: HashMap<String, Arc<RwLock<TrieNode>>>,
    /// Inodes that this path resolves to (usually 1, but could be more for hard links)
    inodes: Vec<i64>,
    /// Whether this node represents a complete path (not just a prefix)
    is_terminal: bool,
}

impl TrieNode {
    fn new() -> Self {
        Self {
            children: HashMap::new(),
            inodes: Vec::new(),
            is_terminal: false,
        }
    }
}

/// Path Trie for efficient path-to-inode mapping and prefix-based invalidation
///
/// # Architecture
///
/// Instead of storing flat path strings, we use a prefix tree (Trie) where:
/// - Each node represents a path component (e.g., "dir1", "file.txt")
/// - Paths like "/a/b/c" are stored as: root → "a" → "b" → "c"
/// - Terminal nodes store the inode numbers they resolve to
///
/// - Shared prefixes: "/a/b/c1" and "/a/b/c2" share "/a/b" nodes
/// - Automatic cleanup: Removing "/a" removes all "/a/*" children
/// - No orphaned entries: Tree structure ensures consistency
struct PathTrie {
    root: Arc<RwLock<TrieNode>>,
}

impl PathTrie {
    fn new() -> Self {
        Self {
            root: Arc::new(RwLock::new(TrieNode::new())),
        }
    }

    /// Inserts a path-to-inode mapping into the trie
    ///
    /// # Arguments
    ///
    /// * `path` - The absolute path (e.g., "/a/b/c")
    /// * `ino` - The inode number this path resolves to
    ///
    /// O(depth) where depth = number of path components
    async fn insert(&self, path: &str, ino: i64) {
        let components = Self::split_path(path);
        let mut current = self.root.clone();

        for component in components {
            let mut node = current.write().await;
            let next = node
                .children
                .entry(component.to_string())
                .or_insert_with(|| Arc::new(RwLock::new(TrieNode::new())))
                .clone();
            drop(node);
            current = next;
        }

        let mut terminal_node = current.write().await;
        terminal_node.is_terminal = true;
        if !terminal_node.inodes.contains(&ino) {
            terminal_node.inodes.push(ino);
        }
    }

    /// Retrieves all inodes associated with a specific path
    ///
    /// # Returns
    ///
    /// * `Some(Vec<i64>)` - List of inodes if path exists
    /// * `None` - If path not found in trie
    #[allow(unused)]
    async fn get(&self, path: &str) -> Option<Vec<i64>> {
        let components = Self::split_path(path);
        let mut current = self.root.clone();

        for component in components {
            let node = current.read().await;
            let next = node.children.get(component).cloned();
            drop(node);

            current = next?;
        }

        let node = current.read().await;
        if node.is_terminal && !node.inodes.is_empty() {
            Some(node.inodes.clone())
        } else {
            None
        }
    }

    /// Removes a path and all its descendants from the trie
    ///
    /// # Arguments
    ///
    /// * `path` - The path prefix to remove (e.g., "/a/b")
    ///
    /// # Returns
    ///
    /// Vector of (path, inodes) tuples for all removed paths
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Trie contains: /a, /a/b, /a/b/c, /a/b/d
    /// trie.remove_by_prefix("/a/b").await;
    /// // Removes: /a/b, /a/b/c, /a/b/d
    /// // Keeps: /a
    /// ```
    async fn remove_by_prefix(&self, path: &str) -> Vec<(String, Vec<i64>)> {
        let components = Self::split_path(path);

        if components.is_empty() {
            // Removing root - clear everything
            let mut root = self.root.write().await;
            let removed = Self::collect_all_paths_from_node(&root, "").await;
            root.children.clear();
            root.inodes.clear();
            root.is_terminal = false;
            return removed;
        }

        // Navigate to parent of target node
        let mut current = self.root.clone();
        for (i, component) in components.iter().enumerate() {
            let node = current.read().await;

            if i == components.len() - 1 {
                // This is the parent of the node we want to remove
                let child = node.children.get(*component).cloned();
                drop(node);

                if let Some(child_node) = child {
                    // Collect all paths in this subtree before removing
                    let child_guard = child_node.read().await;
                    let removed = Self::collect_all_paths_from_node(&child_guard, path).await;
                    drop(child_guard);

                    // Remove the subtree
                    let mut parent = current.write().await;
                    parent.children.remove(*component);

                    return removed;
                }
                return Vec::new();
            }

            let next = node.children.get(*component).cloned();
            drop(node);

            if let Some(next_node) = next {
                current = next_node;
            } else {
                return Vec::new(); // Path not found
            }
        }

        Vec::new()
    }

    /// Recursively collects all paths in a subtree
    ///
    /// # Arguments
    ///
    /// * `node` - The root node of the subtree
    /// * `prefix` - The path prefix leading to this node
    ///
    /// # Returns
    ///
    /// Vector of (path, inodes) tuples for all complete paths in the subtree
    #[async_recursion]
    async fn collect_all_paths_from_node(node: &TrieNode, prefix: &str) -> Vec<(String, Vec<i64>)> {
        let mut paths = Vec::new();

        // If this node is terminal, add its path with inodes
        if node.is_terminal {
            paths.push((prefix.to_string(), node.inodes.clone()));
        }

        // Recursively collect from children
        for (component, child_arc) in &node.children {
            let child = child_arc.read().await;
            let child_prefix = if prefix.is_empty() {
                format!("/{}", component)
            } else {
                format!("{}/{}", prefix, component)
            };

            let child_paths = Self::collect_all_paths_from_node(&child, &child_prefix).await;
            paths.extend(child_paths);
        }

        paths
    }

    /// Splits a path into components for trie navigation
    ///
    /// # Example
    ///
    /// ```ignore
    /// split_path("/a/b/c") => ["a", "b", "c"]
    /// split_path("/") => []
    /// split_path("/foo") => ["foo"]
    /// ```
    fn split_path(path: &str) -> Vec<&str> {
        path.trim_start_matches('/')
            .split('/')
            .filter(|s| !s.is_empty())
            .collect()
    }

    /// Clears all entries from the trie
    #[allow(unused)]
    async fn clear(&self) {
        let mut root = self.root.write().await;
        root.children.clear();
        root.inodes.clear();
        root.is_terminal = false;
    }
}

#[derive(Clone)]
enum ChildrenState {
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
    fn is_complete(&self) -> bool {
        matches!(self, ChildrenState::Complete(_))
    }

    /// Get the underlying map if available (both Partial and Complete)
    fn get_map(&self) -> Option<&Arc<ChildrenMap>> {
        match self {
            ChildrenState::Partial(map) | ChildrenState::Complete(map) => Some(map),
            ChildrenState::NotLoaded => None,
        }
    }

    /// Get mutable access to create or modify the map
    fn get_or_insert_partial(&mut self) -> &Arc<ChildrenMap> {
        match self {
            ChildrenState::Partial(map) | ChildrenState::Complete(map) => map,
            ChildrenState::NotLoaded => {
                *self = ChildrenState::Partial(Arc::new(DashMap::new()));
                match self {
                    ChildrenState::Partial(map) => map,
                    _ => unreachable!(),
                }
            }
        }
    }
}

/// Inode metadata entry in cache
///
/// Represents a cached inode with mutable fields that can be updated in-place.
/// Uses RwLock for async-safe concurrent access to mutable data.
///
/// # Structure
///
/// Each `InodeEntry` caches metadata for a single inode (file or directory):
/// - **`attr`**: Attributes of THIS inode (size, permissions, timestamps, etc.)
/// - **`parent`**: Parent directory's inode number (for upward traversal to build paths)
/// - **`children`**: If THIS inode is a directory, stores its child entries (for readdir)
///
/// # Example
///
/// For a file `/a/b/file.txt` with inode 100, parent directory `/a/b` with inode 50:
/// ```ignore
/// InodeEntry {
///     attr: FileAttr { ino: 100, kind: File, ... },  // THIS file's attributes
///     parent: Some(50),                               // Parent directory's inode
///     children: NotLoaded,                            // Files don't have children
/// }
/// ```
///
/// For a directory `/a/b` with inode 50, parent `/a` with inode 10, containing `file.txt` (ino 100):
/// ```ignore
/// InodeEntry {
///     attr: FileAttr { ino: 50, kind: Directory, ... },  // THIS directory's attributes
///     parent: Some(10),                                   // Parent directory's inode (/a)
///     children: Complete({ "file.txt" -> 100 }),          // THIS directory's children
/// }
/// ```
#[derive(Clone)]
struct InodeEntry {
    /// File attributes (size, permissions, timestamps, etc.) of THIS inode
    attr: Arc<RwLock<FileAttr>>,
    /// Parent directory's inode number (None for root directory)
    /// Used for upward path traversal: child -> parent -> grandparent -> ... -> root
    parent: Arc<RwLock<Option<i64>>>,
    /// Directory children with loading state tracking (only used if THIS inode is a directory)
    /// Maps child names to their inode numbers for readdir operations
    children: Arc<RwLock<ChildrenState>>,
}

impl InodeEntry {
    fn new(attr: FileAttr, parent: Option<i64>) -> Self {
        Self {
            attr: Arc::new(RwLock::new(attr)),
            parent: Arc::new(RwLock::new(parent)),
            children: Arc::new(RwLock::new(ChildrenState::NotLoaded)),
        }
    }

    async fn get_parent(&self) -> Option<i64> {
        *self.parent.read().await
    }

    async fn set_parent(&self, parent_ino: i64) {
        *self.parent.write().await = Some(parent_ino);
    }
}

/// Dual-layer inode cache: Moka for TTL/LRU + DashMap for concurrent mutations
///
/// Architecture:
/// - `entries`: DashMap for lock-free concurrent read/write access to cached data
/// - `ttl_manager`: Moka cache for automatic expiration and capacity management
struct InodeCache {
    entries: Arc<DashMap<i64, Arc<InodeEntry>>>,
    ttl_manager: Cache<i64, Arc<InodeEntry>>,
}

impl InodeCache {
    /// Creates a new InodeCache with specified capacity and TTL.
    ///
    /// # Arguments
    ///
    /// * `capacity` - Maximum number of inode entries to cache
    /// * `ttl` - Time-to-live duration for cached entries
    ///
    /// # Design
    ///
    /// Uses a dual-layer architecture:
    /// - `entries`: DashMap for zero-copy in-place mutations
    /// - `ttl_manager`: Moka cache for automatic TTL-based eviction and LRU
    fn new(capacity: u64, ttl: Duration) -> Self {
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

    /// Inserts or updates an inode node in the cache.
    ///
    /// # Arguments
    ///
    /// * `ino` - The inode number
    /// * `attr` - File attributes for this inode
    /// * `parent` - Optional parent inode number
    ///
    /// # Behavior
    ///
    /// - If node exists: Updates attributes and parent, preserves children
    /// - If node doesn't exist: Creates a new node with children unloaded (None)
    ///
    /// # When to Call
    ///
    /// 1. **After loading from database**: Cache the inode metadata
    ///    ```ignore
    ///    let attr = store.stat(ino).await?;
    ///    cache.insert_node(ino, attr, Some(parent_ino));
    ///    ```
    ///
    /// 2. **After file/directory creation**: Cache the newly created inode
    ///    ```ignore
    ///    let new_ino = store.create_file(parent, name).await?;
    ///    let attr = store.stat(new_ino).await?;
    ///    cache.insert_node(new_ino, attr, Some(parent));
    ///    ```
    ///
    /// 3. **After attribute changes**: Update cached attributes
    ///    ```ignore
    ///    store.set_file_size(ino, new_size).await?;
    ///    let updated_attr = store.stat(ino).await?;
    ///    cache.insert_node(ino, updated_attr, None); // parent unchanged
    ///    ```
    ///
    /// 4. **After moving a file/directory** (rename to different parent): Update parent
    ///    ```ignore
    ///    // Move /a/file.txt to /b/file.txt
    ///    store.rename(old_parent, name, new_parent, name).await?;
    ///    // Only the moved item's parent changes, children don't need updates
    ///    cache.insert_node(file_ino, attr, Some(new_parent));
    ///    ```
    ///
    /// # Note
    ///
    /// When moving a directory, you **do NOT** need to recursively update its children's
    /// parent pointers. Each inode only tracks its direct parent. For example:
    /// - Move `/a/dir` to `/b/dir`: Only update `dir`'s parent (from `a` to `b`)
    /// - Files in `/b/dir/file.txt`: Their parent is still `dir` (unchanged)
    /// - Paths are computed dynamically by traversing parent links
    async fn insert_node(&self, ino: i64, attr: FileAttr, parent: Option<i64>) {
        // Check if node already exists
        if let Some(existing_node) = self.ttl_manager.get(&ino).await {
            // Node exists, only update attr and parent, preserve children
            *existing_node.attr.write().await = attr;
            if let Some(p) = parent {
                existing_node.set_parent(p).await;
            }
        } else {
            // Node doesn't exist, create new one
            let node = Arc::new(InodeEntry::new(attr, parent));
            self.entries.insert(ino, node.clone());
            self.ttl_manager.insert(ino, node).await;
        }
    }

    /// Ensures a node is loaded in cache, fetching from store if necessary.
    ///
    /// # Arguments
    ///
    /// * `ino` - The inode number to ensure is cached
    /// * `store` - Reference to the metadata store for fallback queries
    /// * `parent` - Optional parent inode number (for new nodes)
    ///
    /// # Returns
    ///
    /// * `Ok(true)` - Node was already in cache or successfully loaded
    /// * `Ok(false)` - Node doesn't exist in store
    /// * `Err(MetaError)` - Database error occurred
    ///
    /// # When to Call
    ///
    /// Use this helper to reduce boilerplate code when you need to ensure
    /// a node exists in cache before performing operations on it:
    async fn ensure_node_in_cache(
        &self,
        ino: i64,
        store: &Arc<dyn MetaStore + Send + Sync>,
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

    /// Incrementally adds a child entry to a parent directory's children map.
    ///
    /// # Arguments
    ///
    /// * `parent_ino` - The parent directory's inode number
    /// * `name` - The name of the child entry
    /// * `child_ino` - The child's inode number
    ///
    /// # Behavior
    ///
    /// - Creates Partial state if children not yet loaded
    /// - Adds to existing map (preserves state - Partial or Complete)
    ///
    /// # When to Call
    ///
    /// 1. **After creating a file/directory**: Add to parent's children
    ///    ```ignore
    ///    let new_ino = store.create_file(parent, "file.txt").await?;
    ///    cache.add_child(parent, "file.txt".to_string(), new_ino);
    ///    ```
    ///
    /// 2. **After lookup from database**: Cache the parent-child relationship
    ///    ```ignore
    ///    let child_ino = store.lookup(parent, "name").await?;
    ///    cache.add_child(parent, "name".to_string(), child_ino);
    ///    ```
    ///
    /// 3. **After rename (move to new parent)**: Add to new parent's children
    ///    ```ignore
    ///    store.rename(old_parent, "file", new_parent, "file").await?;
    ///    cache.remove_child(old_parent, "file");  // Remove from old parent
    ///    cache.add_child(new_parent, "file".to_string(), ino);  // Add to new parent
    ///    ```
    ///
    /// # Important
    ///
    /// Entries added to NotLoaded or Partial state remain Partial, meaning a subsequent
    /// readdir will reload from the database to ensure completeness.
    async fn add_child(&self, parent_ino: i64, name: String, child_ino: i64) {
        if let Some(parent_node) = self.ttl_manager.get(&parent_ino).await {
            let mut children_lock = parent_node.children.write().await;
            let children_map = children_lock.get_or_insert_partial();
            children_map.insert(name, child_ino);
        }
    }

    /// Loads a complete set of children entries from the database.
    ///
    /// # Arguments
    ///
    /// * `parent_ino` - The parent directory's inode number
    /// * `entries` - Complete list of (name, child_ino) tuples from database
    ///
    /// # Behavior
    ///
    /// - Replaces any existing children map with fresh data from database
    /// - Marks children as Complete (fully loaded)
    /// - This ensures subsequent readdir calls will use the cache without reloading
    ///
    /// # When to Call
    ///
    /// 1. **After readdir from database**: Load the complete directory listing
    ///    ```ignore
    ///    let entries = store.readdir(dir_ino).await?;
    ///    let children_data: Vec<(String, i64)> =
    ///        entries.iter().map(|e| (e.name.clone(), e.ino)).collect();
    ///    cache.load_children(dir_ino, children_data);
    ///    ```
    ///
    /// 2. **To refresh stale Partial state**: When you know the cache might be incomplete
    ///    ```ignore
    ///    // If previous operations used add_child (Partial state)
    ///    // but now we need the complete listing
    ///    let full_listing = store.readdir(parent).await?;
    ///    cache.load_children(parent, full_listing);
    ///    ```
    ///
    /// # Important
    ///
    /// This is the **only** method that marks children as Complete. It's the authoritative
    /// source that ensures cache consistency with the database.
    async fn load_children(&self, parent_ino: i64, entries: Vec<(String, i64)>) {
        if let Some(parent_node) = self.ttl_manager.get(&parent_ino).await {
            let mut children_lock = parent_node.children.write().await;

            // Create new children map and populate with database data
            let new_children = Arc::new(DashMap::new());
            for (name, child_ino) in entries {
                new_children.insert(name, child_ino);
            }

            // Replace with fresh data from database and mark as Complete
            *children_lock = ChildrenState::Complete(new_children);
        }
    }

    /// Removes a child entry from a parent directory's children map.
    ///
    /// # Arguments
    ///
    /// * `parent_ino` - The parent directory's inode number
    /// * `name` - The name of the child entry to remove
    ///
    /// # Returns
    ///
    /// * `Some(i64)` - The inode number of the removed child
    /// * `None` - If the parent doesn't exist or the child name isn't found
    ///
    /// # When to Call
    ///
    /// 1. **After deleting a file**: Remove from parent's children
    ///    ```ignore
    ///    store.unlink(parent, "file.txt").await?;
    ///    cache.remove_child(parent, "file.txt");
    ///    ```
    ///
    /// 2. **After deleting a directory**: Remove from parent's children
    ///    ```ignore
    ///    store.rmdir(parent, "dirname").await?;
    ///    cache.remove_child(parent, "dirname");
    ///    ```
    ///
    /// 3. **After rename (move from old parent)**: Remove from old parent
    ///    ```ignore
    ///    store.rename(old_parent, "file", new_parent, "newname").await?;
    ///    let child_ino = cache.remove_child(old_parent, "file").unwrap();
    ///    cache.add_child(new_parent, "newname".to_string(), child_ino);
    ///    ```
    ///
    /// # Side Effects
    ///
    /// - Removes the child from the parent's children map
    /// - Invalidates the child's inode cache entry (triggers TTL eviction)
    async fn remove_child(&self, parent_ino: i64, name: &str) -> Option<i64> {
        if let Some(parent_node) = self.ttl_manager.get(&parent_ino).await {
            let children_lock = parent_node.children.read().await;
            if let Some(children_map) = children_lock.get_map() {
                let child_ino = children_map.remove(name).map(|(_, v)| v);

                if let Some(ino) = child_ino {
                    self.ttl_manager.invalidate(&ino).await;
                }

                return child_ino;
            }
        }
        None
    }

    /// Looks up a child inode by name in a parent directory.
    ///
    /// # Arguments
    ///
    /// * `parent_ino` - The parent directory's inode number
    /// * `name` - The name of the child to look up
    ///
    /// # Returns
    ///
    /// * `Some(i64)` - The child's inode number if found in cache
    /// * `None` - If parent doesn't exist, children not loaded, or name not found
    ///
    /// # When to Call
    ///
    /// 1. **Before database lookup**: Try cache first for performance
    ///    ```ignore
    ///    if let Some(ino) = cache.lookup(parent, "file.txt") {
    ///        return Ok(Some(ino));  // Cache hit!
    ///    }
    ///    // Cache miss, query database
    ///    let ino = store.lookup(parent, "file.txt").await?;
    ///    ```
    ///
    /// 2. **During path resolution**: Walk the path using cached lookups
    ///    ```ignore
    ///    let mut current = root_ino;
    ///    for segment in path.split('/') {
    ///        if let Some(child) = cache.lookup(current, segment) {
    ///            current = child;  // Continue with cached data
    ///        } else {
    ///            // Load from database
    ///        }
    ///    }
    ///    ```
    ///
    /// # Note
    ///
    /// This searches cached children in both Partial and Complete states.
    /// Returns None if children are NotLoaded or name doesn't exist.
    async fn lookup(&self, parent_ino: i64, name: &str) -> Option<i64> {
        let parent_node = self.ttl_manager.get(&parent_ino).await?;
        let children_lock = parent_node.children.read().await;
        let children_map = children_lock.get_map()?;
        children_map.get(name).map(|entry| *entry.value())
    }

    /// Reads directory contents from cache.
    ///
    /// # Arguments
    ///
    /// * `ino` - The directory's inode number
    ///
    /// # Returns
    ///
    /// * `Some(Vec<DirEntry>)` - List of directory entries if fully loaded in cache
    /// * `None` - If directory not in cache, or children not fully loaded from DB
    ///
    /// # When to Call
    ///
    /// 1. **Before database readdir**: Try cache first
    ///    ```ignore
    ///    if let Some(entries) = cache.readdir(dir_ino) {
    ///        return Ok(entries);  // Cache hit!
    ///    }
    ///    // Cache miss, load from database
    ///    let entries = store.readdir(dir_ino).await?;
    ///    cache.load_children(dir_ino, entries);
    ///    ```
    ///
    /// 2. **To verify cache completeness**: Check if we can serve from cache
    ///    ```ignore
    ///    match cache.readdir(ino) {
    ///        Some(entries) => /* Use cached data */,
    ///        None => /* Need to reload from DB */,
    ///    }
    ///    ```
    ///
    /// # Behavior
    ///
    /// - Only returns cached entries if state is **Complete**
    /// - Returns None for Partial or NotLoaded states
    /// - This ensures we don't return incomplete listings when entries were only
    ///   added incrementally via add_child without a full database load
    ///
    /// # Performance
    ///
    /// For each cached child, queries its FileType from the inode cache.
    /// If a child's attributes aren't cached, defaults to FileType::File.
    async fn readdir(&self, ino: i64) -> Option<Vec<DirEntry>> {
        let node = self.ttl_manager.get(&ino).await?;
        let children_lock = node.children.read().await;

        // Only return cached results if children have been fully loaded from database
        if !children_lock.is_complete() {
            return None;
        }

        let children_map = children_lock.get_map()?;
        let mut entries = Vec::new();

        for entry in children_map.iter() {
            let (name, child_ino) = entry.pair();

            // Query child node's type from its attributes
            let kind = if let Some(child) = self.ttl_manager.get(child_ino).await {
                child.attr.read().await.kind
            } else {
                // Fallback: assume it's a file if not found in cache
                // TODO: May think about save more data in parent?
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

    /// Retrieves file attributes from cache.
    ///
    /// # Arguments
    ///
    /// * `ino` - The inode number
    ///
    /// # Returns
    ///
    /// * `Some(FileAttr)` - Cached file attributes if found
    /// * `None` - If inode not in cache
    ///
    /// # When to Call
    ///
    /// 1. **Before database stat**: Try cache first
    ///    ```ignore
    ///    if let Some(attr) = cache.get_attr(ino) {
    ///        return Ok(Some(attr));  // Cache hit!
    ///    }
    ///    // Cache miss, query database
    ///    let attr = store.stat(ino).await?;
    ///    ```
    ///
    /// 2. **To check file type**: Get cached metadata
    ///    ```ignore
    ///    if let Some(attr) = cache.get_attr(ino) {
    ///        if attr.kind == FileType::Directory {
    ///            // It's a directory
    ///        }
    ///    }
    ///    ```
    ///
    /// 3. **After modify operations**: Verify cached state
    ///    ```ignore
    ///    cache.insert_node(ino, new_attr, None);
    ///    let cached = cache.get_attr(ino).unwrap();
    ///    assert_eq!(cached.size, new_size);
    ///    ```
    async fn get_attr(&self, ino: i64) -> Option<FileAttr> {
        let node = self.ttl_manager.get(&ino).await?;
        Some(node.attr.read().await.clone())
    }

    /// Updates file attributes in cache (in-place modification).
    ///
    /// # Arguments
    ///
    /// * `ino` - The inode number
    /// * `attr` - New file attributes to set
    ///
    /// # Note
    ///
    /// Only updates the cache; does not persist to underlying storage.
    #[allow(unused)]
    async fn update_attr(&self, ino: i64, attr: FileAttr) {
        if let Some(node) = self.ttl_manager.get(&ino).await {
            *node.attr.write().await = attr;
        }
    }

    /// Retrieves the cached inode node (full entry with all fields).
    ///
    /// # Arguments
    ///
    /// * `ino` - The inode number
    ///
    /// # Returns
    ///
    /// * `Some(Arc<InodeEntry>)` - Shared reference to the cached inode node
    /// * `None` - If inode not in cache
    ///
    /// # When to Call
    ///
    /// 1. **To access parent information**: Get the parent inode
    ///    ```ignore
    ///    if let Some(node) = cache.get_node(ino) {
    ///        let parent = node.get_parent().await;
    ///    }
    ///    ```
    ///
    /// 2. **To check if inode is cached**: Before loading from database
    ///    ```ignore
    ///    if cache.get_node(parent).is_none() {
    ///        // Parent not in cache, need to load it first
    ///        let parent_attr = store.stat(parent).await?;
    ///        cache.insert_node(parent, parent_attr, None);
    ///    }
    ///    ```
    ///
    /// 3. **To update parent pointer**: After rename/move
    ///    ```ignore
    ///    if let Some(child_node) = cache.get_node(child_ino) {
    ///        child_node.set_parent(new_parent).await;
    ///    }
    ///    ```
    ///
    /// 4. **To traverse parent chain**: Build full path
    ///    ```ignore
    ///    let mut current = ino;
    ///    while let Some(node) = cache.get_node(current) {
    ///        if let Some(parent) = node.get_parent().await {
    ///            current = parent;
    ///        } else {
    ///            break;  // Reached root
    ///        }
    ///    }
    ///    ```
    async fn get_node(&self, ino: i64) -> Option<Arc<InodeEntry>> {
        self.ttl_manager.get(&ino).await
    }

    /// Invalidates all cached entries (emergency cache clear).
    ///
    /// # Warning
    ///
    /// This completely clears both the lifecycle cache and storage map.
    /// Should only be used in testing or catastrophic failure scenarios.
    #[allow(unused)]
    fn invalidate_all(&self) {
        self.ttl_manager.invalidate_all();
        self.entries.clear();
    }
}

/// Metadata client with intelligent caching
///
/// This client wraps a MetaStore and provides transparent caching for:
/// - Inode attributes (file metadata)
/// - Directory children (directory listings)
/// - Path-to-inode mappings (path resolution)
pub struct MetaClient {
    store: Arc<dyn MetaStore + Send + Sync>,
    inode_cache: InodeCache,
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
}

impl MetaClient {
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
    pub fn new(
        store: Arc<dyn MetaStore + Send + Sync>,
        capacity: CacheCapacity,
        ttl: CacheTtl,
    ) -> Self {
        Self {
            store,
            inode_cache: InodeCache::new(capacity.inode as u64, ttl.inode_ttl),
            path_cache: Cache::builder()
                .max_capacity(capacity.path as u64)
                .time_to_live(ttl.path_ttl)
                .build(),
            path_trie: Arc::new(PathTrie::new()),
            inode_to_paths: Arc::new(DashMap::new()),
        }
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

    /// Resolves a file path to its corresponding inode number.
    ///
    /// This method walks through the path components from root to leaf,
    /// utilizing both inode cache and path cache for performance optimization.
    ///
    /// # Arguments
    ///
    /// * `path` - The absolute path to resolve (must start with '/')
    ///
    /// # Returns
    ///
    /// * `Ok(i64)` - The inode number of the file/directory
    /// * `Err(MetaError::NotFound)` - If any component in the path doesn't exist
    /// * `Err(MetaError::...)` - Other metadata errors
    pub async fn resolve_path(&self, path: &str) -> Result<i64, MetaError> {
        info!("MetaClient: Resolving path: {}", path);

        if path == "/" {
            return Ok(self.store.root_ino());
        }

        if let Some(ino) = self.path_cache.get(path).await {
            info!("MetaClient: Path cache HIT for '{}' -> inode {}", path, ino);
            return Ok(ino);
        }

        info!("MetaClient: Path cache MISS for '{}'", path);

        let mut current_ino = self.store.root_ino();
        let segments: Vec<&str> = path
            .trim_start_matches('/')
            .split('/')
            .filter(|s| !s.is_empty())
            .collect();

        for seg in segments {
            let child_ino = self
                .cached_lookup(current_ino, seg)
                .await?
                .ok_or_else(|| MetaError::NotFound(current_ino))?;

            current_ino = child_ino;
        }

        // Cache the resolved path in both caches
        self.path_cache.insert(path.to_string(), current_ino).await;

        // Store in trie for efficient prefix-based invalidation
        self.path_trie.insert(path, current_ino).await;

        // Maintain reverse index: inode -> paths
        self.inode_to_paths
            .entry(current_ino)
            .or_default()
            .push(path.to_string());

        Ok(current_ino)
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
        info!("MetaClient: stat request for inode {}", ino);

        if let Some(attr) = self.inode_cache.get_attr(ino).await {
            info!("MetaClient: Inode cache HIT for inode {}", ino);
            return Ok(Some(attr));
        }

        info!("MetaClient: Inode cache MISS for inode {}", ino);

        let attr = self.store.stat(ino).await?;

        if let Some(ref a) = attr {
            info!("MetaClient: Caching attr for inode {}", ino);
            self.inode_cache.insert_node(ino, a.clone(), None).await;
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
        info!("MetaClient: lookup request for ({}, '{}')", parent, name);

        if let Some(ino) = self.inode_cache.lookup(parent, name).await {
            info!(
                "MetaClient: Inode cache HIT for ({}, '{}') -> inode {}",
                parent, name, ino
            );
            return Ok(Some(ino));
        }

        info!("MetaClient: Inode cache MISS for ({}, '{}')", parent, name);

        let result = self.store.lookup(parent, name).await?;

        if let Some(ino) = result {
            info!(
                "MetaClient: Caching lookup result ({}, '{}') -> inode {}",
                parent, name, ino
            );
            if let Ok(Some(attr)) = self.store.stat(ino).await {
                self.inode_cache.insert_node(ino, attr, Some(parent)).await;
            }
            self.inode_cache
                .add_child(parent, name.to_string(), ino)
                .await;
        }

        Ok(result)
    }
}

#[async_trait]
impl MetaStore for MetaClient {
    async fn stat(&self, ino: i64) -> Result<Option<FileAttr>, MetaError> {
        self.cached_stat(ino).await
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
        info!("MetaClient: readdir request for inode {}", ino);

        if let Some(entries) = self.inode_cache.readdir(ino).await {
            info!(
                "MetaClient: Inode cache HIT for readdir inode {} ({} entries)",
                ino,
                entries.len()
            );
            return Ok(entries);
        }

        info!("MetaClient: Inode cache MISS for readdir inode {}", ino);

        let entries = self.store.readdir(ino).await?;

        info!(
            "MetaClient: Caching readdir result for inode {} ({} entries)",
            ino,
            entries.len()
        );

        // Ensure parent directory node is in cache before loading children
        self.inode_cache
            .ensure_node_in_cache(ino, &self.store, None)
            .await?;

        // Load all children from database into cache, replacing any stale data
        let children_data: Vec<(String, i64)> =
            entries.iter().map(|e| (e.name.clone(), e.ino)).collect();
        self.inode_cache.load_children(ino, children_data).await;

        // Also cache each child's attributes
        for entry in &entries {
            if let Ok(Some(attr)) = self.store.stat(entry.ino).await {
                self.inode_cache
                    .insert_node(entry.ino, attr, Some(ino))
                    .await;
            }
        }

        Ok(entries)
    }

    async fn mkdir(&self, parent: i64, name: String) -> Result<i64, MetaError> {
        info!("MetaClient: mkdir operation for ({}, '{}')", parent, name);

        let ino = self.store.mkdir(parent, name.clone()).await?;

        info!("MetaClient: mkdir created inode {}, updating cache", ino);

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
        info!("MetaClient: rmdir operation for ({}, '{}')", parent, name);

        self.store.rmdir(parent, name).await?;

        info!("MetaClient: rmdir completed, updating cache");

        self.inode_cache.remove_child(parent, name).await;
        self.invalidate_parent_path(parent).await;

        Ok(())
    }

    async fn create_file(&self, parent: i64, name: String) -> Result<i64, MetaError> {
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

        // Cache the new file node
        if let Ok(Some(attr)) = self.store.stat(ino).await {
            self.inode_cache.insert_node(ino, attr, Some(parent)).await;
        }
        self.inode_cache.add_child(parent, name, ino).await;

        self.invalidate_parent_path(parent).await;

        Ok(ino)
    }

    async fn unlink(&self, parent: i64, name: &str) -> Result<(), MetaError> {
        info!("MetaClient: unlink operation for ({}, '{}')", parent, name);

        self.store.unlink(parent, name).await?;

        info!("MetaClient: unlink completed, updating cache");

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
        info!(
            "MetaClient: rename operation from ({}, '{}') to ({}, '{}')",
            old_parent, old_name, new_parent, new_name
        );

        self.store
            .rename(old_parent, old_name, new_parent, new_name.clone())
            .await?;

        info!("MetaClient: rename completed, updating cache");

        if let Some(child_ino) = self.inode_cache.remove_child(old_parent, old_name).await {
            // Ensure new parent node is in cache before adding child
            self.inode_cache
                .ensure_node_in_cache(new_parent, &self.store, None)
                .await?;

            self.inode_cache
                .add_child(new_parent, new_name, child_ino)
                .await;

            if let Some(child_node) = self.inode_cache.get_node(child_ino).await {
                child_node.set_parent(new_parent).await;
            }
        }

        // Invalidate both old and new parent paths since both directories changed
        self.invalidate_parent_path(old_parent).await;
        if old_parent != new_parent {
            self.invalidate_parent_path(new_parent).await;
        }

        Ok(())
    }

    async fn set_file_size(&self, ino: i64, size: u64) -> Result<(), MetaError> {
        self.store.set_file_size(ino, size).await?;

        // Update cached attribute
        if let Some(node) = self.inode_cache.get_node(ino).await {
            let mut attr = node.attr.write().await;
            attr.size = size;
        }

        Ok(())
    }

    async fn get_parent(&self, ino: i64) -> Result<Option<i64>, MetaError> {
        if let Some(node) = self.inode_cache.get_node(ino).await
            && let Some(parent) = node.get_parent().await
        {
            return Ok(Some(parent));
        }

        self.store.get_parent(ino).await
    }

    async fn get_name(&self, ino: i64) -> Result<Option<String>, MetaError> {
        if let Some(parent_ino) = self.get_parent(ino).await?
            && let Some(parent_node) = self.inode_cache.get_node(parent_ino).await
        {
            let children_lock = parent_node.children.read().await;
            if let Some(children_map) = children_lock.get_map() {
                for entry in children_map.iter() {
                    if *entry.value() == ino {
                        return Ok(Some(entry.key().clone()));
                    }
                }
            }
        }

        self.store.get_name(ino).await
    }

    async fn get_path(&self, ino: i64) -> Result<Option<String>, MetaError> {
        if ino == self.store.root_ino() {
            return Ok(Some("/".to_string()));
        }

        let node = self.inode_cache.get_node(ino).await;
        if node.is_none() {
            return self.store.get_path(ino).await;
        }

        let mut path_segments = Vec::new();
        let mut current_ino = ino;

        while current_ino != self.store.root_ino() {
            let current_node = self.inode_cache.get_node(current_ino).await;
            if current_node.is_none() {
                return self.store.get_path(ino).await;
            }

            let parent_ino = current_node.as_ref().unwrap().get_parent().await;
            if parent_ino.is_none() {
                return self.store.get_path(ino).await;
            }

            let parent = parent_ino.unwrap();
            let parent_node_opt = self.inode_cache.get_node(parent).await;
            if parent_node_opt.is_none() {
                return self.store.get_path(ino).await;
            }

            let parent_node = parent_node_opt.unwrap();
            let mut found_name = None;
            let children_lock = parent_node.children.read().await;
            if let Some(children_map) = children_lock.get_map() {
                for entry in children_map.iter() {
                    if *entry.value() == current_ino {
                        found_name = Some(entry.key().clone());
                        break;
                    }
                }
            }

            if found_name.is_none() {
                return self.store.get_path(ino).await;
            }

            path_segments.push(found_name.unwrap());
            current_ino = parent;
        }

        path_segments.reverse();
        Ok(Some(format!("/{}", path_segments.join("/"))))
    }

    fn root_ino(&self) -> i64 {
        self.store.root_ino()
    }

    async fn initialize(&self) -> Result<(), MetaError> {
        self.store.initialize().await
    }

    async fn get_deleted_files(&self) -> Result<Vec<i64>, MetaError> {
        self.store.get_deleted_files().await
    }

    async fn remove_file_metadata(&self, ino: i64) -> Result<(), MetaError> {
        self.store.remove_file_metadata(ino).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meta::config::{CacheConfig, Config, DatabaseConfig, DatabaseType};
    use crate::meta::stores::database_store::DatabaseMetaStore;
    use std::time::Duration;

    async fn create_test_client() -> MetaClient {
        create_test_client_with_capacity(100, 100).await
    }

    async fn create_test_client_with_capacity(
        inode_capacity: usize,
        path_capacity: usize,
    ) -> MetaClient {
        let db_path = "sqlite::memory:".to_string();

        let config = Config {
            database: DatabaseConfig {
                db_config: DatabaseType::Sqlite { url: db_path },
            },
            cache: CacheConfig::default(),
        };

        let store = DatabaseMetaStore::from_config(config).await.unwrap();
        let store = Arc::new(store) as Arc<dyn MetaStore + Send + Sync>;

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

    /// Test scenario: Call readdir immediately after creating files to verify fully_loaded flag handling
    ///
    /// Steps:
    /// 1. Create multiple files (children state is Partial at this point)
    /// 2. Call readdir (should load complete list from database, set state to Complete)
    /// 3. Call readdir again (should hit cache)
    /// 4. Create new file (incremental update to already fully loaded cache)
    /// 5. Call readdir again (should contain all files including newly created one)
    #[tokio::test]
    async fn test_readdir_after_incremental_creates() {
        let client = create_test_client().await;

        // Step 1: Create initial files
        let file1 = client
            .create_file(1, "file1.txt".to_string())
            .await
            .unwrap();
        let file2 = client
            .create_file(1, "file2.txt".to_string())
            .await
            .unwrap();

        // At this point root's children are in Partial state
        // because they were only added incrementally via add_child

        // Step 2: First readdir - should load complete list from database
        let entries = client.readdir(1).await.unwrap();
        assert_eq!(entries.len(), 2, "First readdir should return 2 files");

        // Verify returned files
        let names: Vec<String> = entries.iter().map(|e| e.name.clone()).collect();
        assert!(
            names.contains(&"file1.txt".to_string()),
            "Should contain file1.txt"
        );
        assert!(
            names.contains(&"file2.txt".to_string()),
            "Should contain file2.txt"
        );

        // Step 3: Second readdir - should hit cache
        let entries2 = client.readdir(1).await.unwrap();
        assert_eq!(
            entries2.len(),
            2,
            "Second readdir should return same 2 files"
        );

        // Step 4: Create new file
        let file3 = client
            .create_file(1, "file3.txt".to_string())
            .await
            .unwrap();

        // Step 5: Third readdir - should contain all 3 files
        let entries3 = client.readdir(1).await.unwrap();
        assert_eq!(entries3.len(), 3, "Third readdir should return all 3 files");

        let names3: Vec<String> = entries3.iter().map(|e| e.name.clone()).collect();
        assert!(
            names3.contains(&"file1.txt".to_string()),
            "Should contain file1.txt"
        );
        assert!(
            names3.contains(&"file2.txt".to_string()),
            "Should contain file2.txt"
        );
        assert!(
            names3.contains(&"file3.txt".to_string()),
            "Should contain file3.txt"
        );

        // Verify all files can be found via lookup
        assert_eq!(client.lookup(1, "file1.txt").await.unwrap(), Some(file1));
        assert_eq!(client.lookup(1, "file2.txt").await.unwrap(), Some(file2));
        assert_eq!(client.lookup(1, "file3.txt").await.unwrap(), Some(file3));
    }

    /// Test scenario: Create and traverse nested directories
    ///
    /// Directory structure:
    /// /
    /// ├── projects/
    /// │   ├── rust/
    /// │   │   ├── main.rs
    /// │   │   └── lib.rs
    /// │   └── python/
    /// │       └── app.py
    /// └── docs/
    ///     └── README.md
    #[tokio::test]
    async fn test_nested_directory_operations() {
        let client = create_test_client().await;

        // Create directory tree
        let projects = client.mkdir(1, "projects".to_string()).await.unwrap();
        let docs = client.mkdir(1, "docs".to_string()).await.unwrap();

        let rust_dir = client.mkdir(projects, "rust".to_string()).await.unwrap();
        let python_dir = client.mkdir(projects, "python".to_string()).await.unwrap();

        // Create files in each directory
        let main_rs = client
            .create_file(rust_dir, "main.rs".to_string())
            .await
            .unwrap();
        let _lib_rs = client
            .create_file(rust_dir, "lib.rs".to_string())
            .await
            .unwrap();
        let app_py = client
            .create_file(python_dir, "app.py".to_string())
            .await
            .unwrap();
        let readme = client
            .create_file(docs, "README.md".to_string())
            .await
            .unwrap();

        // Test root directory
        let root_entries = client.readdir(1).await.unwrap();
        assert_eq!(root_entries.len(), 2, "Root should have 2 directories");
        let root_names: Vec<String> = root_entries.iter().map(|e| e.name.clone()).collect();
        assert!(root_names.contains(&"projects".to_string()));
        assert!(root_names.contains(&"docs".to_string()));

        // Test projects directory
        let projects_entries = client.readdir(projects).await.unwrap();
        assert_eq!(
            projects_entries.len(),
            2,
            "projects/ should have 2 subdirectories"
        );
        let projects_names: Vec<String> = projects_entries.iter().map(|e| e.name.clone()).collect();
        assert!(projects_names.contains(&"rust".to_string()));
        assert!(projects_names.contains(&"python".to_string()));

        // Test rust directory
        let rust_entries = client.readdir(rust_dir).await.unwrap();
        assert_eq!(rust_entries.len(), 2, "rust/ should have 2 files");
        let rust_names: Vec<String> = rust_entries.iter().map(|e| e.name.clone()).collect();
        assert!(rust_names.contains(&"main.rs".to_string()));
        assert!(rust_names.contains(&"lib.rs".to_string()));

        // Test path resolution
        let resolved_main = client.resolve_path("/projects/rust/main.rs").await.unwrap();
        assert_eq!(resolved_main, main_rs, "Should resolve to correct inode");

        let resolved_readme = client.resolve_path("/docs/README.md").await.unwrap();
        assert_eq!(resolved_readme, readme, "Should resolve to correct inode");

        // Test lookup_path
        let (ino, kind) = client
            .lookup_path("/projects/python/app.py")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(ino, app_py, "lookup_path should return correct inode");
        assert_eq!(kind, FileType::File, "Should be a file");

        // Test get_path reverse lookup
        let main_path = client.get_path(main_rs).await.unwrap().unwrap();
        assert_eq!(
            main_path, "/projects/rust/main.rs",
            "Should resolve path from inode"
        );
    }

    /// Test scenario: File and directory deletion operations
    ///
    /// Verify:
    /// 1. After deleting a file, readdir no longer shows it
    /// 2. lookup should return None
    /// 3. After deleting a directory, parent's readdir no longer shows it
    #[tokio::test]
    async fn test_delete_operations() {
        let client = create_test_client().await;

        // Create test structure
        let dir1 = client.mkdir(1, "dir1".to_string()).await.unwrap();
        let _file1 = client
            .create_file(dir1, "file1.txt".to_string())
            .await
            .unwrap();
        let _file2 = client
            .create_file(dir1, "file2.txt".to_string())
            .await
            .unwrap();
        let _file3 = client
            .create_file(dir1, "file3.txt".to_string())
            .await
            .unwrap();

        // Call readdir to load complete cache
        let entries = client.readdir(dir1).await.unwrap();
        assert_eq!(entries.len(), 3, "Should have 3 files before deletion");

        // Delete one file
        client.unlink(dir1, "file2.txt").await.unwrap();

        // readdir should only show remaining files
        let entries_after = client.readdir(dir1).await.unwrap();
        assert_eq!(entries_after.len(), 2, "Should have 2 files after deletion");

        let names: Vec<String> = entries_after.iter().map(|e| e.name.clone()).collect();
        assert!(names.contains(&"file1.txt".to_string()));
        assert!(names.contains(&"file3.txt".to_string()));
        assert!(
            !names.contains(&"file2.txt".to_string()),
            "Deleted file should not appear"
        );

        // lookup for deleted file should return None
        let lookup_result = client.lookup(dir1, "file2.txt").await.unwrap();
        assert_eq!(
            lookup_result, None,
            "Lookup deleted file should return None"
        );

        // Delete all files
        client.unlink(dir1, "file1.txt").await.unwrap();
        client.unlink(dir1, "file3.txt").await.unwrap();

        // Directory should be empty
        let empty_entries = client.readdir(dir1).await.unwrap();
        assert_eq!(empty_entries.len(), 0, "Directory should be empty");

        // Delete empty directory
        client.rmdir(1, "dir1").await.unwrap();

        // Root should no longer contain dir1
        let root_entries = client.readdir(1).await.unwrap();
        assert_eq!(root_entries.len(), 0, "Root should be empty");

        let lookup_dir = client.lookup(1, "dir1").await.unwrap();
        assert_eq!(lookup_dir, None, "Deleted directory should not be found");
    }

    /// Test scenario: File and directory rename operations
    ///
    /// Verify:
    /// 1. Rename within same directory
    /// 2. Move across directories
    /// 3. Path resolution works correctly after rename
    /// 4. Cache is properly updated
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

        // Verify get_path
        let path = client.get_path(file1).await.unwrap().unwrap();
        assert_eq!(
            path, "/dir2/moved_file.txt",
            "get_path should return new path"
        );
    }

    /// Test scenario: Complex sequence of mixed operations
    ///
    /// Simulate real-world usage: mixed operations of create, read, modify, delete
    #[tokio::test]
    async fn test_complex_mixed_operations() {
        let client = create_test_client().await;

        // Phase 1: Build initial structure
        let src = client.mkdir(1, "src".to_string()).await.unwrap();
        let tests = client.mkdir(1, "tests".to_string()).await.unwrap();

        let _main_rs = client
            .create_file(src, "main.rs".to_string())
            .await
            .unwrap();
        let _lib_rs = client.create_file(src, "lib.rs".to_string()).await.unwrap();
        let _test1 = client
            .create_file(tests, "test1.rs".to_string())
            .await
            .unwrap();

        // Phase 2: Read and verify
        let root_entries = client.readdir(1).await.unwrap();
        assert_eq!(root_entries.len(), 2, "Root should have src and tests");

        let src_entries = client.readdir(src).await.unwrap();
        assert_eq!(src_entries.len(), 2, "src should have 2 files");

        // Phase 3: Add more files
        let _utils_rs = client
            .create_file(src, "utils.rs".to_string())
            .await
            .unwrap();
        let _test2 = client
            .create_file(tests, "test2.rs".to_string())
            .await
            .unwrap();

        // Phase 4: Verify readdir after incremental updates
        let src_entries2 = client.readdir(src).await.unwrap();
        assert_eq!(src_entries2.len(), 3, "src should now have 3 files");

        let tests_entries = client.readdir(tests).await.unwrap();
        assert_eq!(tests_entries.len(), 2, "tests should have 2 files");

        // Phase 5: Rename operations
        client
            .rename(src, "utils.rs", src, "helpers.rs".to_string())
            .await
            .unwrap();

        // Verify rename
        let src_entries3 = client.readdir(src).await.unwrap();
        let src_names: Vec<String> = src_entries3.iter().map(|e| e.name.clone()).collect();
        assert!(src_names.contains(&"helpers.rs".to_string()));
        assert!(!src_names.contains(&"utils.rs".to_string()));

        // Phase 6: Delete operations
        client.unlink(tests, "test1.rs").await.unwrap();

        let tests_entries2 = client.readdir(tests).await.unwrap();
        assert_eq!(
            tests_entries2.len(),
            1,
            "tests should have 1 file after deletion"
        );

        // Phase 7: Create subdirectory
        let models = client.mkdir(src, "models".to_string()).await.unwrap();
        let user_rs = client
            .create_file(models, "user.rs".to_string())
            .await
            .unwrap();

        // Verify multi-level path
        let resolved = client.resolve_path("/src/models/user.rs").await.unwrap();
        assert_eq!(resolved, user_rs);

        // Phase 8: Final verification - check entire tree structure
        let final_root = client.readdir(1).await.unwrap();
        assert_eq!(final_root.len(), 2, "Root should still have 2 directories");

        let final_src = client.readdir(src).await.unwrap();
        assert_eq!(final_src.len(), 4, "src should have 3 files + 1 directory");

        let models_entries = client.readdir(models).await.unwrap();
        assert_eq!(models_entries.len(), 1, "models should have 1 file");
    }

    /// Test scenario: Verify cache consistency between lookup and resolve_path
    ///
    /// Ensure cache remains consistent when accessing the same path via different methods
    #[tokio::test]
    async fn test_lookup_vs_resolve_path_consistency() {
        let client = create_test_client().await;

        // Create deeply nested structure
        let a = client.mkdir(1, "a".to_string()).await.unwrap();
        let b = client.mkdir(a, "b".to_string()).await.unwrap();
        let c = client.mkdir(b, "c".to_string()).await.unwrap();
        let file = client.create_file(c, "deep.txt".to_string()).await.unwrap();

        // Method 1: Step-by-step lookup
        let a_lookup = client.lookup(1, "a").await.unwrap().unwrap();
        let b_lookup = client.lookup(a_lookup, "b").await.unwrap().unwrap();
        let c_lookup = client.lookup(b_lookup, "c").await.unwrap().unwrap();
        let file_lookup = client.lookup(c_lookup, "deep.txt").await.unwrap().unwrap();

        // Method 2: Direct resolve_path
        let file_resolved = client.resolve_path("/a/b/c/deep.txt").await.unwrap();

        // Both methods should return same inode
        assert_eq!(
            file_lookup, file_resolved,
            "lookup and resolve_path should return same inode"
        );
        assert_eq!(file_resolved, file, "Both should match original inode");

        // Method 3: lookup_path
        let (file_lookup_path, kind) = client
            .lookup_path("/a/b/c/deep.txt")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(file_lookup_path, file, "lookup_path should match");
        assert_eq!(kind, FileType::File);

        // Verify cache hit
        assert!(
            client.path_cache.get("/a/b/c/deep.txt").await.is_some(),
            "Path should be cached"
        );
    }

    /// Test scenario: Cache consistency under high concurrency
    ///
    /// Simulate interleaved operations and verify cache remains consistent
    #[tokio::test]
    async fn test_interleaved_operations() {
        let client = create_test_client().await;

        let dir1 = client.mkdir(1, "dir1".to_string()).await.unwrap();

        // Create files without calling readdir (children not fully loaded)
        let f1 = client
            .create_file(dir1, "f1.txt".to_string())
            .await
            .unwrap();
        let _f2 = client
            .create_file(dir1, "f2.txt".to_string())
            .await
            .unwrap();

        // Access via lookup (doesn't trigger readdir)
        let f1_lookup = client.lookup(dir1, "f1.txt").await.unwrap().unwrap();
        assert_eq!(f1_lookup, f1);

        // Create more files
        let _f3 = client
            .create_file(dir1, "f3.txt".to_string())
            .await
            .unwrap();

        // Now call readdir - should load complete list from database
        let entries = client.readdir(dir1).await.unwrap();
        assert_eq!(entries.len(), 3, "Should see all 3 files created");

        // Verify all files exist
        let names: Vec<String> = entries.iter().map(|e| e.name.clone()).collect();
        assert!(names.contains(&"f1.txt".to_string()));
        assert!(names.contains(&"f2.txt".to_string()));
        assert!(names.contains(&"f3.txt".to_string()));

        // Create another file (children now fully loaded)
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

    /// Test scenario: get_parent and get_name functionality
    ///
    /// Verify reverse lookup from inode to parent directory and name
    #[tokio::test]
    async fn test_get_parent_and_name() {
        let client = create_test_client().await;

        let dir1 = client.mkdir(1, "parent_dir".to_string()).await.unwrap();
        let file1 = client
            .create_file(dir1, "child_file.txt".to_string())
            .await
            .unwrap();

        // Test get_parent
        let parent = client.get_parent(file1).await.unwrap().unwrap();
        assert_eq!(parent, dir1, "Parent should be parent_dir");

        let root_parent = client.get_parent(dir1).await.unwrap().unwrap();
        assert_eq!(root_parent, 1, "Parent of dir1 should be root");

        // Test get_name
        let file_name = client.get_name(file1).await.unwrap().unwrap();
        assert_eq!(file_name, "child_file.txt", "Name should match");

        let dir_name = client.get_name(dir1).await.unwrap().unwrap();
        assert_eq!(dir_name, "parent_dir", "Directory name should match");

        let root_name = client.get_name(1).await.unwrap();
        assert_eq!(
            root_name,
            Some("/".to_string()),
            "Root directory name should be '/'"
        );
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
}
