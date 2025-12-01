//! FUSE/SDK-friendly VFS with path-based create/mkdir/read/write/readdir/stat support.

use crate::chuck::chunk::ChunkLayout;
use crate::chuck::reader::ChunkReader;
use crate::chuck::store::BlockStore;
use crate::chuck::writer::ChunkWriter;
use crate::meta::client::{MetaClient, MetaClientOptions};
use crate::meta::config::{CacheCapacity, CacheTtl};
use crate::meta::store::MetaError;
use crate::meta::{MetaLayer, MetaStore};
use dashmap::{DashMap, Entry};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

// Re-export types from meta::store for convenience
pub use crate::meta::store::{DirEntry, FileAttr, FileType};
use crate::vfs::handles::{DirHandle, FileHandle, HandleFlags};
use crate::vfs::inode::Inode;
use crate::vfs::io::FileRegistry;

struct HandleRegistry {
    handles: DashMap<i64, Vec<FileHandle>>,
    handle_ino: DashMap<u64, i64>,
    dir_handles: DashMap<u64, Arc<DirHandle>>,
    next_fh: AtomicU64,
}

impl HandleRegistry {
    fn new() -> Self {
        Self {
            handles: DashMap::new(),
            handle_ino: DashMap::new(),
            dir_handles: DashMap::new(),
            next_fh: AtomicU64::new(1),
        }
    }

    async fn allocate(&self, ino: i64, flags: HandleFlags) -> u64 {
        let fh = self.next_fh.fetch_add(1, Ordering::Relaxed);
        self.handles
            .entry(ino)
            .or_default()
            .push(FileHandle::new(fh, flags));
        self.handle_ino.insert(fh, ino);
        fh
    }

    async fn release(&self, fh: u64) -> Option<FileHandle> {
        let ino = self.handle_ino.remove(&fh)?.1;
        let mut entry = self.handles.get_mut(&ino)?;
        if let Some(idx) = entry.iter().position(|h| h.fh == fh) {
            let handle = entry.remove(idx);
            let empty = entry.is_empty();
            drop(entry);
            if empty {
                self.handles.remove(&ino);
            }
            tracing::info!("release file handle: fh={}, ino={}", fh, ino);
            Some(handle)
        } else {
            None
        }
    }

    async fn with_handle_mut<R>(&self, fh: u64, f: impl FnOnce(&mut FileHandle) -> R) -> Option<R> {
        let ino = *self.handle_ino.get(&fh)?.value();
        let mut entry = self.handles.get_mut(&ino)?;
        entry.iter_mut().find(|h| h.fh == fh).map(f)
    }

    async fn handles_for(&self, ino: i64) -> Vec<u64> {
        self.handles
            .get(&ino)
            .map(|entry| entry.iter().map(|h| h.fh).collect())
            .unwrap_or_default()
    }

    async fn allocate_dir(&self, handle: DirHandle) -> u64 {
        let fh = self.next_fh.fetch_add(1, Ordering::Relaxed);
        self.dir_handles.insert(fh, Arc::new(handle));
        fh
    }

    async fn release_dir(&self, fh: u64) -> Option<Arc<DirHandle>> {
        self.dir_handles.remove(&fh).map(|(_, handle)| handle)
    }

    async fn get_dir(&self, fh: u64) -> Option<Arc<DirHandle>> {
        self.dir_handles
            .get(&fh)
            .map(|entry| Arc::clone(entry.value()))
    }
}

struct ModifiedTracker {
    entries: Mutex<HashMap<i64, Instant>>,
}

impl ModifiedTracker {
    fn new() -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
        }
    }

    async fn touch(&self, ino: i64) {
        let mut guard = self.entries.lock().await;
        guard.insert(ino, Instant::now());
    }

    async fn modified_since(&self, ino: i64, since: Instant) -> bool {
        let guard = self.entries.lock().await;
        guard.get(&ino).map(|&ts| ts >= since).unwrap_or(false)
    }

    async fn cleanup_older_than(&self, ttl: Duration) {
        let now = Instant::now();
        let cutoff = now.checked_sub(ttl).unwrap_or(now);
        let mut guard = self.entries.lock().await;
        guard.retain(|_, ts| *ts >= cutoff);
    }
}

pub struct ChunkIoFactory<S, M>
where
    S: BlockStore,
    M: MetaStore,
{
    layout: ChunkLayout,
    store: Arc<S>,
    meta: Arc<M>,
}

impl<S, M> ChunkIoFactory<S, M>
where
    S: BlockStore,
    M: MetaStore,
{
    fn new(layout: ChunkLayout, store: Arc<S>, meta: Arc<M>) -> Self {
        Self {
            layout,
            store,
            meta,
        }
    }

    pub fn layout(&self) -> ChunkLayout {
        self.layout
    }

    pub fn reader(&self, chunk_id: u64) -> ChunkReader<'_, S, M> {
        ChunkReader::new(
            self.layout,
            chunk_id,
            self.store.as_ref(),
            self.meta.as_ref(),
        )
    }

    pub fn writer(&self, chunk_id: u64) -> ChunkWriter<'_, S, M> {
        ChunkWriter::new(
            self.layout,
            chunk_id,
            self.store.as_ref(),
            self.meta.as_ref(),
        )
    }
}

struct VfsState<S, M>
where
    S: BlockStore,
    M: MetaStore + 'static,
{
    handles: HandleRegistry,
    files: FileRegistry<S, M>,
    modified: ModifiedTracker,
}

impl<S, M> VfsState<S, M>
where
    S: BlockStore,
    M: MetaStore + 'static,
{
    fn new() -> Self {
        Self {
            handles: HandleRegistry::new(),
            files: FileRegistry::new(),
            modified: ModifiedTracker::new(),
        }
    }
}

#[allow(dead_code)]
pub struct VfsCore<S, M>
where
    S: BlockStore,
    M: MetaStore + 'static,
{
    layout: ChunkLayout,
    meta_layer: Arc<MetaClient<M>>,
    chunk_io: Arc<ChunkIoFactory<S, M>>,
    root: i64,
}

impl<S, M> VfsCore<S, M>
where
    S: BlockStore,
    M: MetaStore + 'static,
{
    fn new(
        layout: ChunkLayout,
        store: Arc<S>,
        meta_store: Arc<M>,
        meta_layer: Arc<MetaClient<M>>,
        root: i64,
    ) -> Self {
        let chunk_io = Arc::new(ChunkIoFactory::new(
            layout,
            Arc::clone(&store),
            Arc::clone(&meta_store),
        ));
        Self {
            layout,
            meta_layer,
            chunk_io,
            root,
        }
    }
}

#[derive(Debug, Clone)]
pub struct MetaClientConfig {
    pub capacity: CacheCapacity,
    pub ttl: CacheTtl,
    pub options: MetaClientOptions,
}

impl Default for MetaClientConfig {
    fn default() -> Self {
        Self {
            capacity: CacheCapacity::default(),
            ttl: CacheTtl::for_sqlite(),
            options: MetaClientOptions::default(),
        }
    }
}

#[allow(unused)]
#[allow(clippy::upper_case_acronyms)]
#[derive(Clone)]
pub struct VFS<S, M>
where
    S: BlockStore,
    M: MetaStore + 'static,
{
    core: Arc<VfsCore<S, M>>,
    state: Arc<VfsState<S, M>>,
}

#[allow(dead_code)]
impl<S, M> VFS<S, M>
where
    S: BlockStore,
    M: MetaStore + 'static,
{
    pub async fn new(layout: ChunkLayout, store: S, meta: M) -> Result<Self, String> {
        Self::with_meta_client_config(layout, store, meta, MetaClientConfig::default()).await
    }

    pub async fn with_meta_layer(
        layout: ChunkLayout,
        store: S,
        meta: M,
        meta_layer: Arc<MetaClient<M>>,
    ) -> Result<Self, String> {
        let store = Arc::new(store);
        let meta = Arc::new(meta);
        Self::from_components(layout, store, meta, meta_layer)
    }

    fn from_components(
        layout: ChunkLayout,
        store: Arc<S>,
        meta: Arc<M>,
        meta_layer: Arc<MetaClient<M>>,
    ) -> Result<Self, String> {
        let root_ino = meta_layer.root_ino();
        let core = Arc::new(VfsCore::new(
            layout,
            Arc::clone(&store),
            Arc::clone(&meta),
            meta_layer,
            root_ino,
        ));
        let state = Arc::new(VfsState::new());

        Ok(Self { core, state })
    }

    pub async fn with_meta_client_config(
        layout: ChunkLayout,
        store: S,
        meta: M,
        config: MetaClientConfig,
    ) -> Result<Self, String> {
        let store = Arc::new(store);
        let meta = Arc::new(meta);

        let ttl = if config.ttl.is_zero() {
            CacheTtl::for_sqlite()
        } else {
            config.ttl.clone()
        };

        let meta_client = MetaClient::with_options(
            Arc::clone(&meta),
            config.capacity.clone(),
            ttl,
            config.options.clone(),
        );

        meta_client.initialize().await.map_err(|e| e.to_string())?;

        let meta_layer: Arc<MetaClient<M>> = meta_client.clone();

        Self::from_components(layout, store, meta, meta_layer)
    }

    pub fn root_ino(&self) -> i64 {
        self.core.root
    }

    /// get the node's parent inode.
    pub async fn parent_of(&self, ino: i64) -> Option<i64> {
        self.core.meta_layer.get_parent(ino).await.ok().flatten()
    }

    /// get the node's fullpath.
    pub async fn path_of(&self, ino: i64) -> Option<String> {
        self.core.meta_layer.get_path(ino).await.ok().flatten()
    }

    /// get the node's child inode by name.
    pub async fn child_of(&self, parent: i64, name: &str) -> Option<i64> {
        self.core
            .meta_layer
            .lookup(parent, name)
            .await
            .ok()
            .flatten()
    }

    pub async fn stat_ino(&self, ino: i64) -> Option<FileAttr> {
        self.core.meta_layer.stat(ino).await.ok().flatten()
    }

    /// List directory entries by inode
    pub async fn readdir_ino(&self, ino: i64) -> Option<Vec<DirEntry>> {
        let meta_entries = self.core.meta_layer.readdir(ino).await.ok()?;

        Some(meta_entries)
    }

    /// Open a directory handle for reading. Returns the file handle ID.
    /// This pre-loads all directory entries to support efficient pagination.
    pub async fn opendir_handle(&self, ino: i64) -> Result<u64, MetaError> {
        // Verify directory exists
        let attr = self
            .core
            .meta_layer
            .stat(ino)
            .await?
            .ok_or(MetaError::NotFound(ino))?;

        if attr.kind != FileType::Dir {
            return Err(MetaError::NotDirectory(ino));
        }

        // Load all directory entries
        let entries = self.core.meta_layer.readdir(ino).await?;

        // Create handle
        let handle = DirHandle::new(ino, entries);
        let fh = self.state.handles.allocate_dir(handle).await;

        Ok(fh)
    }

    /// Close a directory handle
    pub async fn closedir_handle(&self, fh: u64) -> Result<(), MetaError> {
        let handle = self
            .state
            .handles
            .release_dir(fh)
            .await
            .ok_or(MetaError::InvalidHandle(fh))?;

        tracing::info!(
            "release dir handle: fh={}, ino={}, entries={}",
            fh,
            handle.ino,
            handle.entries.len()
        );
        Ok(())
    }

    /// Read directory entries by handle with pagination
    pub async fn readdir_by_handle(&self, fh: u64, offset: u64) -> Option<Vec<DirEntry>> {
        let handle = self.state.handles.get_dir(fh).await?;
        Some(handle.get_entries(offset))
    }

    /// Normalize a path by stripping redundant separators and ensuring it starts with `/`.
    /// Does not resolve `.` or `..`.
    fn norm_path(p: &str) -> String {
        if p.is_empty() {
            return "/".into();
        }
        let parts: Vec<&str> = p.split('/').filter(|s| !s.is_empty()).collect();
        let mut out = String::from("/");
        out.push_str(&parts.join("/"));
        if out.is_empty() { "/".into() } else { out }
    }

    /// Split a normalized path into parent directory and basename.
    fn split_dir_file(path: &str) -> (String, String) {
        let n = path.rfind('/').unwrap_or(0);
        if n == 0 {
            ("/".into(), path[1..].into())
        } else {
            (path[..n].into(), path[n + 1..].into())
        }
    }

    /// Recursively create directories (mkdir -p behavior).
    /// - If an intermediate component exists as a file, return "not a directory".
    /// - Idempotent: existing directories simply return their inode.
    /// - Returns the inode of the target directory.
    pub async fn mkdir_p(&self, path: &str) -> Result<i64, String> {
        let path = Self::norm_path(path);
        if &path == "/" {
            return Ok(self.core.root);
        }
        if let Ok(Some((ino, _attr))) = self.core.meta_layer.lookup_path(&path).await {
            return Ok(ino);
        }
        let mut cur_ino = self.core.root;
        for part in path.trim_start_matches('/').split('/') {
            if part.is_empty() {
                continue;
            }
            match self.core.meta_layer.lookup(cur_ino, part).await {
                Ok(Some(ino)) => {
                    if let Ok(Some(attr)) = self.core.meta_layer.stat(ino).await
                        && attr.kind != FileType::Dir
                    {
                        return Err("not a directory".into());
                    }
                    cur_ino = ino;
                }
                _ => {
                    let ino = self
                        .core
                        .meta_layer
                        .mkdir(cur_ino, part.to_string())
                        .await
                        .map_err(|e| e.to_string())?;
                    self.state.modified.touch(cur_ino).await;
                    self.state.modified.touch(ino).await;
                    cur_ino = ino;
                }
            }
        }
        Ok(cur_ino)
    }

    /// Create a regular file (running `mkdir_p` on its parent if needed).
    /// - If a directory with the same name exists, returns "is a directory".
    /// - If the file already exists, returns its inode instead of creating a new one.
    pub async fn create_file(&self, path: &str) -> Result<i64, String> {
        let path = Self::norm_path(path);
        let (dir, name) = Self::split_dir_file(&path);
        let dir_ino = self.mkdir_p(&dir).await?;

        // check the file exists and then return.
        if let Ok(Some(ino)) = self.core.meta_layer.lookup(dir_ino, &name).await
            && let Ok(Some(attr)) = self.core.meta_layer.stat(ino).await
        {
            return if attr.kind == FileType::Dir {
                Err("is a directory".into())
            } else {
                Ok(ino)
            };
        }

        let ino = self
            .core
            .meta_layer
            .create_file(dir_ino, name.clone())
            .await
            .map_err(|e| e.to_string())?;
        self.state.modified.touch(dir_ino).await;
        self.state.modified.touch(ino).await;
        Ok(ino)
    }

    /// Create a hard link at `link_path` that references `existing_path`.
    pub async fn link(&self, existing_path: &str, link_path: &str) -> Result<FileAttr, String> {
        let existing_path = Self::norm_path(existing_path);
        let link_path = Self::norm_path(link_path);

        if existing_path == "/" {
            return Err("is a directory".into());
        }
        if link_path == "/" {
            return Err("invalid path".into());
        }

        let (src_ino, src_kind) = self
            .core
            .meta_layer
            .lookup_path(&existing_path)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "not found".to_string())?;

        if src_kind == FileType::Dir {
            return Err("is a directory".into());
        }

        let (parent_path, name) = Self::split_dir_file(&link_path);
        if name.is_empty() {
            return Err("invalid name".into());
        }

        let parent_ino = if &parent_path == "/" {
            self.core.root
        } else {
            self.core
                .meta_layer
                .lookup_path(&parent_path)
                .await
                .map_err(|e| e.to_string())?
                .ok_or_else(|| "parent not found".to_string())?
                .0
        };

        let parent_attr = self
            .core
            .meta_layer
            .stat(parent_ino)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "parent not found".to_string())?;
        if parent_attr.kind != FileType::Dir {
            return Err("not a directory".into());
        }

        if self
            .core
            .meta_layer
            .lookup(parent_ino, &name)
            .await
            .map_err(|e| e.to_string())?
            .is_some()
        {
            return Err("already exists".into());
        }

        let attr = self
            .core
            .meta_layer
            .link(src_ino, parent_ino, &name)
            .await
            .map_err(|e| match e {
                MetaError::AlreadyExists { .. } => "already exists".to_string(),
                MetaError::ParentNotFound(_) => "parent not found".to_string(),
                MetaError::NotDirectory(_) => "not a directory".to_string(),
                MetaError::NotFound(_) => "not found".to_string(),
                MetaError::NotSupported(_) | MetaError::NotImplemented => {
                    "not supported".to_string()
                }
                _ => e.to_string(),
            })?;

        self.state.modified.touch(parent_ino).await;
        self.state.modified.touch(src_ino).await;

        Ok(attr)
    }

    /// Create a symbolic link at `link_path` pointing to `target`.
    pub async fn create_symlink(
        &self,
        link_path: &str,
        target: &str,
    ) -> Result<(i64, FileAttr), String> {
        let link_path = Self::norm_path(link_path);
        if link_path == "/" {
            return Err("invalid path".into());
        }
        let (dir, name) = Self::split_dir_file(&link_path);
        if name.is_empty() {
            return Err("invalid name".into());
        }

        let parent_ino = if &dir == "/" {
            self.core.root
        } else {
            self.core
                .meta_layer
                .lookup_path(&dir)
                .await
                .map_err(|e| e.to_string())?
                .ok_or_else(|| "parent not found".to_string())?
                .0
        };

        let parent_attr = self
            .core
            .meta_layer
            .stat(parent_ino)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "parent not found".to_string())?;
        if parent_attr.kind != FileType::Dir {
            return Err("not a directory".into());
        }

        if self
            .core
            .meta_layer
            .lookup(parent_ino, &name)
            .await
            .map_err(|e| e.to_string())?
            .is_some()
        {
            return Err("already exists".into());
        }

        let (ino, attr) = self
            .core
            .meta_layer
            .symlink(parent_ino, &name, target)
            .await
            .map_err(|e| match e {
                MetaError::NotSupported(_) | MetaError::NotImplemented => {
                    "not supported".to_string()
                }
                _ => e.to_string(),
            })?;

        self.state.modified.touch(parent_ino).await;
        self.state.modified.touch(ino).await;

        Ok((ino, attr))
    }

    /// Fetch a file's attributes (kind/size come from the MetaStore); returns None when missing.
    pub async fn stat(&self, path: &str) -> Option<FileAttr> {
        let path = Self::norm_path(path);
        let (ino, _) = self.core.meta_layer.lookup_path(&path).await.ok()??;
        let meta_attr = self.core.meta_layer.stat(ino).await.ok().flatten()?;
        Some(meta_attr)
    }

    /// Read a symlink target by inode.
    pub async fn readlink_ino(&self, ino: i64) -> Result<String, String> {
        let attr = self
            .core
            .meta_layer
            .stat(ino)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "not found".to_string())?;
        if attr.kind != FileType::Symlink {
            return Err("not a symlink".into());
        }

        self.core
            .meta_layer
            .read_symlink(ino)
            .await
            .map_err(|e| match e {
                MetaError::NotSupported(_) | MetaError::NotImplemented => {
                    "not supported".to_string()
                }
                _ => e.to_string(),
            })
    }

    /// Read a symlink target by path.
    pub async fn readlink(&self, path: &str) -> Result<String, String> {
        let path = Self::norm_path(path);
        let (ino, kind) = self
            .core
            .meta_layer
            .lookup_path(&path)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "not found".to_string())?;
        if kind != FileType::Symlink {
            return Err("not a symlink".into());
        }

        self.readlink_ino(ino).await
    }

    /// List directory entries; returns None if the path is missing or not a directory.
    /// `.` and `..` are not included.
    pub async fn readdir(&self, path: &str) -> Option<Vec<DirEntry>> {
        let path = Self::norm_path(path);
        let (ino, _) = self.core.meta_layer.lookup_path(&path).await.ok()??;

        let meta_entries = self.core.meta_layer.readdir(ino).await.ok()?;

        let entries: Vec<DirEntry> = meta_entries
            .into_iter()
            .map(|e| DirEntry {
                name: e.name,
                ino: e.ino,
                kind: e.kind,
            })
            .collect();
        Some(entries)
    }

    /// Check whether a path exists.
    pub async fn exists(&self, path: &str) -> bool {
        let path = Self::norm_path(path);
        matches!(self.core.meta_layer.lookup_path(&path).await, Ok(Some(_)))
    }

    /// Remove a regular file or symlink (directories are not supported here).
    pub async fn unlink(&self, path: &str) -> Result<(), String> {
        let path = Self::norm_path(path);
        let (dir, name) = Self::split_dir_file(&path);

        let parent_ino = if &dir == "/" {
            self.core.root
        } else {
            self.core
                .meta_layer
                .lookup_path(&dir)
                .await
                .map_err(|e| e.to_string())?
                .ok_or_else(|| "parent not found".to_string())?
                .0
        };

        let ino = self
            .core
            .meta_layer
            .lookup(parent_ino, &name)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "not found".to_string())?;

        let attr = self
            .core
            .meta_layer
            .stat(ino)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "not found".to_string())?;

        if attr.kind == FileType::Dir {
            return Err("is a directory".into());
        }

        self.core
            .meta_layer
            .unlink(parent_ino, &name)
            .await
            .map_err(|e| e.to_string())?;
        self.state.modified.touch(parent_ino).await;
        self.state.modified.touch(ino).await;

        Ok(())
    }

    /// Remove an empty directory (root cannot be removed; non-empty dirs error out).
    pub async fn rmdir(&self, path: &str) -> Result<(), String> {
        let path = Self::norm_path(path);
        if path == "/" {
            return Err("cannot remove root".into());
        }

        let (dir, name) = Self::split_dir_file(&path);

        let parent_ino = if &dir == "/" {
            self.core.root
        } else {
            self.core
                .meta_layer
                .lookup_path(&dir)
                .await
                .map_err(|e| e.to_string())?
                .ok_or_else(|| "parent not found".to_string())?
                .0
        };

        let ino = self
            .core
            .meta_layer
            .lookup(parent_ino, &name)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "not found".to_string())?;

        let attr = self
            .core
            .meta_layer
            .stat(ino)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "not found".to_string())?;

        if attr.kind != FileType::Dir {
            return Err("not a directory".into());
        }

        let children = self
            .core
            .meta_layer
            .readdir(ino)
            .await
            .map_err(|e| e.to_string())?;
        if !children.is_empty() {
            return Err("directory not empty".into());
        }

        self.core
            .meta_layer
            .rmdir(parent_ino, &name)
            .await
            .map_err(|e| e.to_string())?;
        self.state.modified.touch(parent_ino).await;
        self.state.modified.touch(ino).await;

        Ok(())
    }

    /// Rename files or directories.
    ///
    /// Implements POSIX rename semantics: if the destination exists, it will be replaced,
    /// subject to appropriate checks (e.g., file/directory type compatibility, non-empty directories).
    /// Parent directories are created as needed.
    pub async fn rename(&self, old: &str, new: &str) -> Result<(), MetaError> {
        let old = Self::norm_path(old);
        let new = Self::norm_path(new);
        let (old_dir, old_name) = Self::split_dir_file(&old);
        let (new_dir, new_name) = Self::split_dir_file(&new);

        // Resolve old parent and source inode/attributes first
        let old_parent_ino = if &old_dir == "/" {
            self.core.root
        } else {
            self.core
                .meta_layer
                .lookup_path(&old_dir)
                .await?
                .ok_or(MetaError::ParentNotFound(0))?
                .0
        };

        let src_ino = self
            .core
            .meta_layer
            .lookup(old_parent_ino, &old_name)
            .await?
            .ok_or(MetaError::NotFound(old_parent_ino))?;

        let src_attr = self
            .core
            .meta_layer
            .stat(src_ino)
            .await?
            .ok_or(MetaError::NotFound(src_ino))?;

        // If destination exists, apply replace semantics:
        // - If dest is file/symlink: unlink it
        // - If dest is dir: source must be dir and dest must be empty; rmdir it
        if let Ok(Some((dest_ino, dest_kind))) = self.core.meta_layer.lookup_path(&new).await {
            // resolve parent directory ino for destination
            let new_dir_ino = if &new_dir == "/" {
                self.core.root
            } else {
                // parent must exist when destination exists
                self.core
                    .meta_layer
                    .lookup_path(&new_dir)
                    .await?
                    .ok_or(MetaError::ParentNotFound(0))?
                    .0
            };

            if dest_kind == FileType::Dir {
                // source must be directory
                if src_attr.kind != FileType::Dir {
                    return Err(MetaError::NotDirectory(dest_ino));
                }

                // ensure destination dir is empty
                let children = self.core.meta_layer.readdir(dest_ino).await?;
                if !children.is_empty() {
                    return Err(MetaError::DirectoryNotEmpty(dest_ino));
                }

                // remove the empty destination directory
                self.core.meta_layer.rmdir(new_dir_ino, &new_name).await?;
            } else {
                if src_attr.kind == FileType::Dir {
                    return Err(MetaError::NotDirectory(dest_ino));
                }
                // dest is a file or symlink: unlink it to allow replace
                self.core.meta_layer.unlink(new_dir_ino, &new_name).await?;
            }
        }

        // Ensure destination parent exists (create as needed)
        let new_dir_ino = self
            .mkdir_p(&new_dir)
            .await
            .map_err(|_| MetaError::ParentNotFound(0))?;

        // Perform rename
        self.core
            .meta_layer
            .rename(old_parent_ino, &old_name, new_dir_ino, new_name)
            .await?;

        self.state.modified.touch(old_parent_ino).await;
        self.state.modified.touch(new_dir_ino).await;

        Ok(())
    }

    /// Truncate/extend file size (metadata only; holes are read as zeros).
    /// Shrinking does not eagerly reclaim block data.
    pub async fn truncate(&self, path: &str, size: u64) -> Result<(), String> {
        let path = Self::norm_path(path);
        let (ino, _) = self
            .core
            .meta_layer
            .lookup_path(&path)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "not found".to_string())?;
        if let Some(inode) = self.state.files.inode(ino) {
            inode.update_size(size);
        }
        self.core
            .meta_layer
            .set_file_size(ino, size)
            .await
            .map_err(|e| e.to_string())?;
        self.state.modified.touch(ino).await;
        Ok(())
    }

    /// Write data by file offset. Internally splits the range into per-chunk writes.
    /// Writes each affected chunk fragment. Updates the file size at the end only if the write extends the file.
    pub async fn write(&self, path: &str, offset: u64, data: &[u8]) -> Result<usize, String> {
        let path = Self::norm_path(path);
        let (ino, kind) = self
            .core
            .meta_layer
            .lookup_path(&path)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "not found".to_string())?;
        if kind != FileType::File {
            return Err("not a file".into());
        }

        let inode = self.ensure_inode_registered(ino).await?;
        let writer = self
            .state
            .files
            .writer(ino)
            .ok_or_else(|| "file writer is not initialized".to_string())?;

        let guard = writer.write().await;
        let written = guard.write(offset, data).await.map_err(|e| e.to_string())?;

        let target_size = offset + data.len() as u64;
        if target_size > inode.file_size() {
            inode.update_size(target_size);
            self.core
                .meta_layer
                .set_file_size(ino, target_size)
                .await
                .map_err(|e| e.to_string())?;
        }
        self.state.modified.touch(ino).await;
        Ok(written)
    }

    /// Read data by file offset.
    /// Read by inode directly
    pub async fn read_ino(&self, ino: i64, offset: u64, len: usize) -> Result<Vec<u8>, String> {
        if len == 0 {
            return Ok(Vec::new());
        }
        self.ensure_inode_registered(ino).await?;
        let reader = self
            .state
            .files
            .reader(ino)
            .ok_or_else(|| "file reader is not initialized".to_string())?;
        reader.read(offset, len).await.map_err(|e| e.to_string())
    }

    /// Read by path (convenience method that uses read_ino internally)
    pub async fn read(&self, path: &str, offset: u64, len: usize) -> Result<Vec<u8>, String> {
        let path = Self::norm_path(path);
        let (ino, _) = self
            .core
            .meta_layer
            .lookup_path(&path)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "not found".to_string())?;
        self.read_ino(ino, offset, len).await
    }

    /// Allocate a per-file handle, returning the opaque fh id.
    pub async fn open_handle(&self, ino: i64, read: bool, write: bool) -> u64 {
        self.state
            .handles
            .allocate(ino, HandleFlags::new(read, write))
            .await
    }

    /// Release a previously allocated file handle.
    pub async fn close_handle(&self, fh: u64) -> Result<(), String> {
        self.state
            .handles
            .release(fh)
            .await
            .map(|_| ())
            .ok_or_else(|| "invalid handle".into())
    }

    /// Update cached information about a handle (e.g. last observed offset).
    pub async fn touch_handle_offset(&self, fh: u64, offset: u64) -> Result<(), String> {
        self.state
            .handles
            .with_handle_mut(fh, |handle| handle.last_offset = offset)
            .await
            .map(|_| ())
            .ok_or_else(|| "invalid handle".into())
    }

    /// List all open handles for an inode.
    pub async fn handles_for(&self, ino: i64) -> Vec<u64> {
        self.state.handles.handles_for(ino).await
    }

    /// Check whether a file has been modified since a given point in time.
    pub async fn modified_since(&self, ino: i64, since: Instant) -> bool {
        self.state.modified.modified_since(ino, since).await
    }

    /// Drop modification markers older than `ttl` to keep the tracker bounded.
    pub async fn cleanup_modified(&self, ttl: Duration) {
        self.state.modified.cleanup_older_than(ttl).await;
    }

    async fn ensure_inode_registered(&self, ino: i64) -> Result<Arc<Inode>, String> {
        // fast path to check whether there is an existing inode.
        if let Some(inode) = self.state.files.inode(ino) {
            return Ok(inode.clone());
        }

        // double-check: lock the entry and do the check again.
        match self.state.files.inode.entry(ino) {
            Entry::Occupied(entry) => Ok(Arc::clone(entry.get())),
            Entry::Vacant(entry) => {
                let attr = self
                    .core
                    .meta_layer
                    .stat(ino)
                    .await
                    .map_err(|e| e.to_string())?
                    .ok_or_else(|| "not found".to_string())?;
                if attr.kind != FileType::File {
                    return Err("not a file".into());
                }

                let inode = Inode::new(ino, attr.size);
                self.state
                    .files
                    .ensure_init(Arc::clone(&inode), Arc::clone(&self.core.chunk_io));
                entry.insert(inode.clone());
                Ok(inode)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cadapter::client::ObjectClient;
    use crate::cadapter::localfs::LocalFsBackend;
    use crate::chuck::store::InMemoryBlockStore;
    use crate::chuck::store::ObjectBlockStore;
    use crate::meta::factory::create_meta_store_from_url;
    use async_trait::async_trait;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use tokio::sync::{Barrier, Notify};
    use tokio::time::{sleep, timeout};

    #[tokio::test]
    async fn test_fs_mkdir_create_write_read_readdir() {
        let layout = ChunkLayout::default();
        let tmp = tempfile::tempdir().unwrap();
        let client = ObjectClient::new(LocalFsBackend::new(tmp.path()));
        let store = ObjectBlockStore::new(client);

        let meta_handle = create_meta_store_from_url("sqlite::memory:").await.unwrap();
        let meta_store = meta_handle.store();
        let fs = VFS::new(layout, store, meta_store).await.unwrap();

        fs.mkdir_p("/a/b").await.expect("mkdir_p");
        fs.create_file("/a/b/hello.txt").await.expect("create");
        let data_len = layout.block_size as usize + (layout.block_size / 2) as usize;
        let mut data = vec![0u8; data_len];
        for (i, b) in data.iter_mut().enumerate().take(data_len) {
            *b = (i % 251) as u8;
        }
        fs.write("/a/b/hello.txt", (layout.block_size / 2) as u64, &data)
            .await
            .expect("write");
        let out = fs
            .read("/a/b/hello.txt", (layout.block_size / 2) as u64, data_len)
            .await
            .expect("read");
        assert_eq!(out, data);

        let entries = fs.readdir("/a/b").await.expect("readdir");
        assert!(
            entries
                .iter()
                .any(|e| e.name == "hello.txt" && e.kind == FileType::File)
        );

        let stat = fs.stat("/a/b/hello.txt").await.unwrap();
        assert_eq!(stat.kind, FileType::File);
        assert!(stat.size >= data_len as u64);
    }

    #[tokio::test]
    async fn test_fs_unlink_rmdir_rename_truncate() {
        let layout = ChunkLayout::default();
        let tmp = tempfile::tempdir().unwrap();
        let client = ObjectClient::new(LocalFsBackend::new(tmp.path()));
        let store = ObjectBlockStore::new(client);

        let meta_handle = create_meta_store_from_url("sqlite::memory:").await.unwrap();
        let meta_store = meta_handle.store();
        let fs = VFS::new(layout, store, meta_store).await.unwrap();

        fs.mkdir_p("/a/b").await.unwrap();
        fs.create_file("/a/b/t.txt").await.unwrap();
        assert!(fs.exists("/a/b/t.txt").await);

        // rename file
        fs.rename("/a/b/t.txt", "/a/b/u.txt").await.unwrap();
        assert!(!fs.exists("/a/b/t.txt").await && fs.exists("/a/b/u.txt").await);

        // truncate
        fs.truncate("/a/b/u.txt", layout.block_size as u64 * 2)
            .await
            .unwrap();
        let st = fs.stat("/a/b/u.txt").await.unwrap();
        assert!(st.size >= (layout.block_size * 2) as u64);

        // unlink and rmdir
        fs.unlink("/a/b/u.txt").await.unwrap();
        assert!(!fs.exists("/a/b/u.txt").await);
        // dir empty then rmdir
        fs.rmdir("/a/b").await.unwrap();
        assert!(!fs.exists("/a/b").await);
    }

    /// When different inodes are written concurrently, each file should make progress without
    /// contending on the registry locks. `BarrierBlockStore` forces the first block write of each
    /// task to wait at a barrier so we know both writes overlapped before they finish.
    #[tokio::test]
    async fn test_fs_parallel_writes_to_distinct_files() {
        let layout = ChunkLayout::default();
        let store = BarrierBlockStore::new();
        let meta_handle = create_meta_store_from_url("sqlite::memory:").await.unwrap();
        let meta_store = meta_handle.store();
        let fs = VFS::new(layout, store, meta_store).await.unwrap();

        fs.create_file("/alpha").await.unwrap();
        fs.create_file("/beta").await.unwrap();
        let payload = vec![7u8; 8];

        let fs_a = fs.clone();
        let task_a = tokio::spawn({
            let data = payload.clone();
            async move {
                fs_a.write("/alpha", 0, &data).await.expect("write alpha");
            }
        });
        let fs_b = fs.clone();
        let task_b = tokio::spawn({
            let data = payload.clone();
            async move {
                fs_b.write("/beta", 0, &data).await.expect("write beta");
            }
        });

        timeout(Duration::from_secs(2), async {
            task_a.await.unwrap();
            task_b.await.unwrap();
        })
        .await
        .expect("writes on different files should not block each other");
    }

    /// Readers of the same inode must be serialized behind in-flight writers to avoid racing
    /// with partial chunk updates. `BlockingBlockStore` pauses the first `write_range` until the
    /// test explicitly releases it; the read should block until then and observe the final data.
    #[tokio::test]
    async fn test_fs_same_file_read_waits_for_active_write() {
        let layout = ChunkLayout::default();
        let (store, controller) = BlockingBlockStore::new();
        let meta_handle = create_meta_store_from_url("sqlite::memory:").await.unwrap();
        let meta_store = meta_handle.store();
        let fs = VFS::new(layout, store, meta_store).await.unwrap();
        fs.create_file("/shared").await.unwrap();
        let payload = vec![3u8; 32];
        let expected = payload.clone();
        let len = expected.len();

        let writer_fs = fs.clone();
        let writer_payload = payload;
        let writer = tokio::spawn(async move {
            writer_fs
                .write("/shared", 0, &writer_payload)
                .await
                .expect("write shared");
        });

        controller.wait_for_block().await;

        let read_finished = Arc::new(AtomicBool::new(false));
        let reader_fs = fs.clone();
        let read_done = read_finished.clone();
        let reader = tokio::spawn(async move {
            let data = reader_fs
                .read("/shared", 0, len)
                .await
                .expect("read shared");
            read_done.store(true, Ordering::SeqCst);
            data
        });

        sleep(Duration::from_millis(50)).await;
        assert!(
            !read_finished.load(Ordering::SeqCst),
            "read should wait for in-flight writer"
        );

        controller.release_block();

        let contents = reader.await.unwrap();
        assert_eq!(contents, expected, "read should observe written bytes");
        writer.await.unwrap();
    }

    /// BlockStore wrapper that synchronizes the first two writes so we can prove inter-file
    /// operations progress concurrently.
    #[derive(Clone)]
    struct BarrierBlockStore {
        inner: Arc<InMemoryBlockStore>,
        gate: Arc<Barrier>,
        calls: Arc<AtomicUsize>,
    }

    impl BarrierBlockStore {
        fn new() -> Self {
            Self {
                inner: Arc::new(InMemoryBlockStore::new()),
                gate: Arc::new(Barrier::new(2)),
                calls: Arc::new(AtomicUsize::new(0)),
            }
        }
    }

    #[async_trait]
    impl BlockStore for BarrierBlockStore {
        async fn write_range(
            &self,
            key: crate::chuck::store::BlockKey,
            offset: u32,
            data: &[u8],
        ) -> anyhow::Result<u64> {
            let idx = self.calls.fetch_add(1, Ordering::SeqCst);
            if idx < 2 {
                self.gate.wait().await;
            }
            self.inner.write_range(key, offset, data).await
        }

        async fn read_range(
            &self,
            key: crate::chuck::store::BlockKey,
            offset: u32,
            buf: &mut [u8],
        ) -> anyhow::Result<()> {
            self.inner.read_range(key, offset, buf).await
        }

        async fn delete_range(
            &self,
            key: crate::chuck::store::BlockKey,
            len: usize,
        ) -> anyhow::Result<()> {
            self.inner.delete_range(key, len).await
        }
    }

    /// BlockStore wrapper that pauses the first write until the test releases it, letting us
    /// simulate a long-running writer on a single inode.
    #[derive(Clone)]
    struct BlockingBlockStore {
        inner: Arc<InMemoryBlockStore>,
        state: Arc<BlockingState>,
    }

    impl BlockingBlockStore {
        fn new() -> (Self, BlockingController) {
            let state = Arc::new(BlockingState::new());
            (
                Self {
                    inner: Arc::new(InMemoryBlockStore::new()),
                    state: state.clone(),
                },
                BlockingController { state },
            )
        }
    }

    struct BlockingState {
        paused: AtomicBool,
        started: Notify,
        release: Notify,
    }

    impl BlockingState {
        fn new() -> Self {
            Self {
                paused: AtomicBool::new(true),
                started: Notify::new(),
                release: Notify::new(),
            }
        }
    }

    /// Small handle that lets tests await the point where the writer is blocked and then release it.
    struct BlockingController {
        state: Arc<BlockingState>,
    }

    impl BlockingController {
        async fn wait_for_block(&self) {
            self.state.started.notified().await;
        }

        fn release_block(&self) {
            self.state.release.notify_waiters();
        }
    }

    #[async_trait]
    impl BlockStore for BlockingBlockStore {
        async fn write_range(
            &self,
            key: crate::chuck::store::BlockKey,
            offset: u32,
            data: &[u8],
        ) -> anyhow::Result<u64> {
            if self.state.paused.swap(false, Ordering::SeqCst) {
                self.state.started.notify_waiters();
                self.state.release.notified().await;
            }
            self.inner.write_range(key, offset, data).await
        }

        async fn read_range(
            &self,
            key: crate::chuck::store::BlockKey,
            offset: u32,
            buf: &mut [u8],
        ) -> anyhow::Result<()> {
            self.inner.read_range(key, offset, buf).await
        }

        async fn delete_range(
            &self,
            key: crate::chuck::store::BlockKey,
            len: usize,
        ) -> anyhow::Result<()> {
            self.inner.delete_range(key, len).await
        }
    }
}
