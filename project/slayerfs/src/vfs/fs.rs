//! FUSE/SDK-friendly VFS with path-based metadata ops and handle-based IO.

use crate::chuck::chunk::ChunkLayout;
use crate::chuck::store::BlockStore;
use crate::meta::client::{MetaClient, MetaClientOptions};
use crate::meta::config::{CacheCapacity, CacheTtl};
use crate::meta::file_lock::{FileLockInfo, FileLockQuery, FileLockRange, FileLockType};
use crate::meta::store::{SetAttrFlags, SetAttrRequest};
use crate::meta::{MetaLayer, MetaStore};
use dashmap::{DashMap, Entry};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

// Re-export types from meta::store for convenience
pub use crate::meta::store::{DirEntry, FileAttr, FileType};
use crate::vfs::backend::Backend;
use crate::vfs::config::VFSConfig;
use crate::vfs::error::{PathHint, VfsError};
use crate::vfs::handles::{DirHandle, FileHandle, HandleFlags};
use crate::vfs::inode::Inode;
use crate::vfs::io::{DataReader, DataWriter};

struct HandleRegistry<B, M>
where
    B: BlockStore + Send + Sync + 'static,
    M: MetaStore + Send + Sync + 'static,
{
    handles: DashMap<u64, Arc<FileHandle<B, M>>>,
    inode_handles: DashMap<i64, Vec<u64>>,
    dir_handles: DashMap<u64, Arc<DirHandle>>,
    next_fh: AtomicU64,
}

impl<B, M> HandleRegistry<B, M>
where
    B: BlockStore + Send + Sync + 'static,
    M: MetaStore + Send + Sync + 'static,
{
    fn new() -> Self {
        Self {
            handles: DashMap::new(),
            inode_handles: DashMap::new(),
            dir_handles: DashMap::new(),
            next_fh: AtomicU64::new(1),
        }
    }

    fn allocate(&self, ino: i64, attr: FileAttr, flags: HandleFlags) -> Arc<FileHandle<B, M>> {
        let fh = self.next_fh.fetch_add(1, Ordering::Relaxed);
        let handle = Arc::new(FileHandle::new(fh, ino, attr, flags));
        self.handles.insert(fh, handle.clone());
        self.inode_handles.entry(ino).or_default().push(fh);
        handle
    }

    fn release(&self, fh: u64) -> Option<(Arc<FileHandle<B, M>>, bool)> {
        let handle = self.handles.remove(&fh)?.1;
        let ino = handle.ino;
        let mut last = false;
        if let Some(mut entry) = self.inode_handles.get_mut(&ino) {
            if let Some(idx) = entry.iter().position(|id| *id == fh) {
                entry.remove(idx);
            }
            let empty = entry.is_empty();
            drop(entry);
            if empty {
                self.inode_handles.remove(&ino);
                last = true;
            }
        }
        Some((handle, last))
    }

    fn get(&self, fh: u64) -> Option<Arc<FileHandle<B, M>>> {
        self.handles.get(&fh).map(|entry| Arc::clone(entry.value()))
    }

    fn handles_for(&self, ino: i64) -> Vec<u64> {
        self.inode_handles
            .get(&ino)
            .map(|entry| entry.value().clone())
            .unwrap_or_default()
    }

    fn attr_for(&self, fh: u64) -> Option<FileAttr> {
        self.handles.get(&fh).map(|entry| entry.attr())
    }

    fn attr_for_inode(&self, ino: i64) -> Option<FileAttr> {
        let fhs = self.handles_for(ino);
        for fh in fhs {
            if let Some(handle) = self.handles.get(&fh) {
                return Some(handle.attr());
            }
        }
        None
    }

    fn update_attr_for_inode(&self, ino: i64, attr: &FileAttr) {
        let fhs = self.handles_for(ino);
        for fh in fhs {
            if let Some(handle) = self.handles.get(&fh) {
                handle.update_attr(attr);
            }
        }
    }

    /// Check if any handle for this inode was opened for writing
    fn has_write_handle(&self, ino: i64) -> bool {
        let fhs = self.handles_for(ino);
        fhs.iter()
            .any(|fh| self.handles.get(fh).map(|h| h.flags.write).unwrap_or(false))
    }

    fn has_no_handle(&self, ino: i64) -> bool {
        self.handles_for(ino).is_empty()
    }

    fn allocate_dir(&self, handle: DirHandle) -> u64 {
        let fh = self.next_fh.fetch_add(1, Ordering::Relaxed);
        self.dir_handles.insert(fh, Arc::new(handle));
        fh
    }

    fn release_dir(&self, fh: u64) -> Option<Arc<DirHandle>> {
        self.dir_handles.remove(&fh).map(|(_, handle)| handle)
    }

    fn get_dir(&self, fh: u64) -> Option<Arc<DirHandle>> {
        self.dir_handles
            .get(&fh)
            .map(|entry| Arc::clone(entry.value()))
    }
}

struct ModifiedTracker {
    entries: Mutex<HashMap<i64, Instant>>,
}

struct ExtendFileSizeStats {
    calls: AtomicU64,
    total_us: AtomicU64,
    max_us: AtomicU64,
}

impl ExtendFileSizeStats {
    fn new() -> Self {
        Self {
            calls: AtomicU64::new(0),
            total_us: AtomicU64::new(0),
            max_us: AtomicU64::new(0),
        }
    }

    fn record(&self, elapsed: Duration) -> Option<(u64, u64, u64)> {
        let us = elapsed.as_micros() as u64;
        let calls = self.calls.fetch_add(1, Ordering::Relaxed) + 1;
        self.total_us.fetch_add(us, Ordering::Relaxed);
        self.max_us.fetch_max(us, Ordering::Relaxed);
        if calls.is_multiple_of(1024) {
            let total = self.total_us.load(Ordering::Relaxed);
            let max = self.max_us.load(Ordering::Relaxed);
            let avg = total / calls.max(1);
            return Some((calls, avg, max));
        }
        None
    }
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

struct VfsState<S, M>
where
    S: BlockStore + Send + Sync + 'static,
    M: MetaStore + Send + Sync + 'static,
{
    handles: HandleRegistry<S, M>,
    inodes: DashMap<i64, Arc<Inode>>,
    reader: Arc<DataReader<S, M>>,
    writer: Arc<DataWriter<S, M>>,
    modified: ModifiedTracker,
    extend_stats: ExtendFileSizeStats,
}

impl<S, M> VfsState<S, M>
where
    S: BlockStore + Send + Sync + 'static,
    M: MetaStore + Send + Sync + 'static,
{
    fn new(config: Arc<VFSConfig>, backend: Arc<Backend<S, M>>) -> Self {
        let reader = Arc::new(DataReader::new(config.read.clone(), backend.clone()));
        let writer = Arc::new(DataWriter::new(
            config.write.clone(),
            backend,
            reader.clone(),
        ));
        writer.start_flush_background();
        Self {
            handles: HandleRegistry::new(),
            inodes: DashMap::new(),
            reader,
            writer,
            modified: ModifiedTracker::new(),
            extend_stats: ExtendFileSizeStats::new(),
        }
    }
}

#[allow(dead_code)]
pub(crate) struct VfsCore<S, M>
where
    S: BlockStore + Send + Sync + 'static,
    M: MetaStore + Send + Sync + 'static,
{
    layout: ChunkLayout,
    backend: Arc<Backend<S, M>>,
    meta_layer: Arc<MetaClient<M>>,
    root: i64,
}

impl<S, M> VfsCore<S, M>
where
    S: BlockStore + Send + Sync + 'static,
    M: MetaStore + Send + Sync + 'static,
{
    pub(crate) fn new(
        layout: ChunkLayout,
        backend: Arc<Backend<S, M>>,
        meta_layer: Arc<MetaClient<M>>,
        root: i64,
    ) -> Self {
        Self {
            layout,
            backend,
            meta_layer,
            root,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct MetaClientConfig {
    pub(crate) capacity: CacheCapacity,
    pub(crate) ttl: CacheTtl,
    pub(crate) options: MetaClientOptions,
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
    S: BlockStore + Send + Sync + 'static,
    M: MetaStore + Send + Sync + 'static,
{
    core: Arc<VfsCore<S, M>>,
    state: Arc<VfsState<S, M>>,
}

#[allow(dead_code)]
impl<S, M> VFS<S, M>
where
    S: BlockStore + Send + Sync + 'static,
    M: MetaStore + Send + Sync + 'static,
{
    pub async fn new(layout: ChunkLayout, store: S, meta: M) -> Result<Self, VfsError> {
        Self::with_meta_client_config(layout, store, meta, MetaClientConfig::default()).await
    }

    pub fn with_config(config: VFSConfig, store: S, meta: M) -> Result<Self, VfsError> {
        let store = Arc::new(store);
        let meta = Arc::new(meta);

        let ttl = if config.meta.ttl.is_zero() {
            CacheTtl::for_sqlite()
        } else {
            config.meta.ttl.clone()
        };

        let meta_client = MetaClient::with_options(
            Arc::clone(&meta),
            config.meta.capacity.clone(),
            ttl,
            config.meta.options.clone(),
        );

        Self::from_components(config, store, meta, meta_client)
    }

    pub(crate) fn with_meta_layer(
        layout: ChunkLayout,
        store: S,
        meta: M,
        meta_layer: Arc<MetaClient<M>>,
    ) -> Result<Self, VfsError> {
        let config = VFSConfig::new(layout);
        let store = Arc::new(store);
        let meta = Arc::new(meta);
        Self::from_components(config, store, meta, meta_layer)
    }

    fn from_components(
        config: VFSConfig,
        store: Arc<S>,
        meta: Arc<M>,
        meta_layer: Arc<MetaClient<M>>,
    ) -> Result<Self, VfsError> {
        let layout = config.write.layout;
        let root_ino = meta_layer.root_ino();
        let backend = Arc::new(Backend::new(store.clone(), meta.clone()));
        let core = Arc::new(VfsCore::new(layout, backend.clone(), meta_layer, root_ino));
        let config = Arc::new(VFSConfig::new(layout));
        let state = Arc::new(VfsState::new(config, backend));

        Ok(Self { core, state })
    }

    pub(crate) async fn with_meta_client_config(
        layout: ChunkLayout,
        store: S,
        meta: M,
        config: MetaClientConfig,
    ) -> Result<Self, VfsError> {
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

        meta_client.initialize().await.map_err(VfsError::from)?;

        let meta_layer: Arc<MetaClient<M>> = meta_client.clone();

        Self::from_components(VFSConfig::new(layout), store, meta, meta_layer)
    }

    pub(crate) fn root_ino(&self) -> i64 {
        self.core.root
    }

    /// get the node's parent inode.
    pub async fn parent_of(&self, ino: i64) -> Option<i64> {
        self.core
            .meta_layer
            .get_dir_parent(ino)
            .await
            .ok()
            .flatten()
    }

    /// get the node's fullpath.
    pub async fn path_of(&self, ino: i64) -> Option<String> {
        self.core
            .meta_layer
            .get_paths(ino)
            .await
            .ok()
            .and_then(|paths| paths.into_iter().next())
    }

    /// get the node's child inode by name.
    pub(crate) async fn child_of(&self, parent: i64, name: &str) -> Option<i64> {
        self.core
            .meta_layer
            .lookup(parent, name)
            .await
            .ok()
            .flatten()
    }

    pub(crate) async fn stat_ino(&self, ino: i64) -> Option<FileAttr> {
        self.core.meta_layer.stat(ino).await.ok().flatten()
    }

    /// Update atime (access time) for an inode to current time
    pub(crate) async fn update_atime(&self, ino: i64) -> Result<(), VfsError> {
        use std::time::{SystemTime, UNIX_EPOCH};

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| VfsError::Other)?
            .as_nanos() as i64;

        let req = SetAttrRequest {
            atime: Some(now),
            ..Default::default()
        };

        self.core
            .meta_layer
            .set_attr(ino, &req, SetAttrFlags::empty())
            .await
            .map_err(VfsError::from)?;

        // Update handle cache if exists
        if let Some(mut attr) = self.state.handles.attr_for_inode(ino) {
            attr.atime = now;
            self.state.handles.update_attr_for_inode(ino, &attr);
        }

        Ok(())
    }

    /// Update mtime and ctime for an inode to current time
    /// This is called during flush/fsync to handle mmap writes where the kernel
    /// doesn't call the write() callback
    pub(crate) async fn update_mtime_ctime(&self, ino: i64) -> Result<(), VfsError> {
        use std::time::{SystemTime, UNIX_EPOCH};

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| VfsError::Other)?
            .as_nanos() as i64;

        let req = SetAttrRequest {
            mtime: Some(now),
            ctime: Some(now),
            ..Default::default()
        };

        self.core
            .meta_layer
            .set_attr(ino, &req, SetAttrFlags::empty())
            .await
            .map_err(VfsError::from)?;

        // Update handle cache if exists
        if let Some(mut attr) = self.state.handles.attr_for_inode(ino) {
            attr.mtime = now;
            attr.ctime = now;
            self.state.handles.update_attr_for_inode(ino, &attr);
        }

        Ok(())
    }

    /// List directory entries by inode
    pub(crate) async fn readdir_ino(&self, ino: i64) -> Option<Vec<DirEntry>> {
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
    pub async fn mkdir_p(&self, path: &str) -> Result<i64, VfsError> {
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
                        return Err(VfsError::NotADirectory {
                            path: PathHint::some(path.clone()),
                        });
                    }
                    cur_ino = ino;
                }
                _ => {
                    let ino = self
                        .core
                        .meta_layer
                        .mkdir(cur_ino, part.to_string())
                        .await
                        .map_err(VfsError::from)?;
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
    pub async fn create_file(&self, path: &str) -> Result<i64, VfsError> {
        let path = Self::norm_path(path);
        let (dir, name) = Self::split_dir_file(&path);
        let dir_ino = self.mkdir_p(&dir).await?;

        // check the file exists and then return.
        if let Ok(Some(ino)) = self.core.meta_layer.lookup(dir_ino, &name).await
            && let Ok(Some(attr)) = self.core.meta_layer.stat(ino).await
        {
            return if attr.kind == FileType::Dir {
                Err(VfsError::IsADirectory {
                    path: PathHint::some(path),
                })
            } else {
                Ok(ino)
            };
        }

        let ino = self
            .core
            .meta_layer
            .create_file(dir_ino, name.clone())
            .await
            .map_err(VfsError::from)?;
        self.state.modified.touch(dir_ino).await;
        self.state.modified.touch(ino).await;
        Ok(ino)
    }

    /// Create a hard link at `link_path` that references `existing_path`.
    pub async fn link(&self, existing_path: &str, link_path: &str) -> Result<FileAttr, VfsError> {
        let existing_path = Self::norm_path(existing_path);
        let link_path = Self::norm_path(link_path);

        if existing_path == "/" {
            return Err(VfsError::IsADirectory {
                path: PathHint::some(existing_path),
            });
        }
        if link_path == "/" {
            return Err(VfsError::InvalidFilename);
        }

        let (src_ino, src_kind) = self
            .core
            .meta_layer
            .lookup_path(&existing_path)
            .await
            .map_err(VfsError::from)?
            .ok_or_else(|| VfsError::NotFound {
                path: PathHint::some(existing_path.clone()),
            })?;

        if src_kind == FileType::Dir {
            return Err(VfsError::IsADirectory {
                path: PathHint::some(existing_path.clone()),
            });
        }

        let (parent_path, name) = Self::split_dir_file(&link_path);
        if name.is_empty() {
            return Err(VfsError::InvalidFilename);
        }

        let parent_ino = if &parent_path == "/" {
            self.core.root
        } else {
            self.core
                .meta_layer
                .lookup_path(&parent_path)
                .await
                .map_err(VfsError::from)?
                .ok_or_else(|| VfsError::NotFound {
                    path: PathHint::some(parent_path.clone()),
                })?
                .0
        };

        let parent_attr = self
            .core
            .meta_layer
            .stat(parent_ino)
            .await
            .map_err(VfsError::from)?
            .ok_or_else(|| VfsError::NotFound {
                path: PathHint::some(parent_path.clone()),
            })?;
        if parent_attr.kind != FileType::Dir {
            return Err(VfsError::NotADirectory {
                path: PathHint::some(parent_path.clone()),
            });
        }

        if self
            .core
            .meta_layer
            .lookup(parent_ino, &name)
            .await
            .map_err(VfsError::from)?
            .is_some()
        {
            return Err(VfsError::AlreadyExists {
                path: PathHint::some(link_path.clone()),
            });
        }

        let attr = self
            .core
            .meta_layer
            .link(src_ino, parent_ino, &name)
            .await
            .map_err(VfsError::from)?;

        self.state.modified.touch(parent_ino).await;
        self.state.modified.touch(src_ino).await;

        Ok(attr)
    }

    /// Create a symbolic link at `link_path` pointing to `target`.
    pub async fn create_symlink(
        &self,
        link_path: &str,
        target: &str,
    ) -> Result<(i64, FileAttr), VfsError> {
        let link_path = Self::norm_path(link_path);
        if link_path == "/" {
            return Err(VfsError::InvalidFilename);
        }
        let (dir, name) = Self::split_dir_file(&link_path);
        if name.is_empty() {
            return Err(VfsError::InvalidFilename);
        }

        let parent_ino = if &dir == "/" {
            self.core.root
        } else {
            self.core
                .meta_layer
                .lookup_path(&dir)
                .await
                .map_err(VfsError::from)?
                .ok_or_else(|| VfsError::NotFound {
                    path: PathHint::some(dir.clone()),
                })?
                .0
        };

        let parent_attr = self
            .core
            .meta_layer
            .stat(parent_ino)
            .await
            .map_err(VfsError::from)?
            .ok_or_else(|| VfsError::NotFound {
                path: PathHint::some(dir.clone()),
            })?;
        if parent_attr.kind != FileType::Dir {
            return Err(VfsError::NotADirectory {
                path: PathHint::some(dir.clone()),
            });
        }

        if self
            .core
            .meta_layer
            .lookup(parent_ino, &name)
            .await
            .map_err(VfsError::from)?
            .is_some()
        {
            return Err(VfsError::AlreadyExists {
                path: PathHint::some(link_path.clone()),
            });
        }

        let (ino, attr) = self
            .core
            .meta_layer
            .symlink(parent_ino, &name, target)
            .await
            .map_err(VfsError::from)?;

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
    pub(crate) async fn readlink_ino(&self, ino: i64) -> Result<String, VfsError> {
        let attr = self
            .core
            .meta_layer
            .stat(ino)
            .await
            .map_err(VfsError::from)?
            .ok_or(VfsError::NotFound {
                path: PathHint::none(),
            })?;
        if attr.kind != FileType::Symlink {
            return Err(VfsError::InvalidInput);
        }

        self.core
            .meta_layer
            .read_symlink(ino)
            .await
            .map_err(VfsError::from)
    }

    /// Read a symlink target by path.
    pub async fn readlink(&self, path: &str) -> Result<String, VfsError> {
        let path = Self::norm_path(path);
        let (ino, kind) = self
            .core
            .meta_layer
            .lookup_path(&path)
            .await
            .map_err(VfsError::from)?
            .ok_or_else(|| VfsError::NotFound {
                path: PathHint::some(path.clone()),
            })?;
        if kind != FileType::Symlink {
            return Err(VfsError::InvalidInput);
        }

        self.readlink_ino(ino).await
    }

    /// Check whether a path exists.
    pub async fn exists(&self, path: &str) -> bool {
        let path = Self::norm_path(path);
        matches!(self.core.meta_layer.lookup_path(&path).await, Ok(Some(_)))
    }

    /// Remove a regular file or symlink (directories are not supported here).
    pub async fn unlink(&self, path: &str) -> Result<(), VfsError> {
        let path = Self::norm_path(path);
        let (dir, name) = Self::split_dir_file(&path);

        let parent_ino = if &dir == "/" {
            self.core.root
        } else {
            self.core
                .meta_layer
                .lookup_path(&dir)
                .await
                .map_err(VfsError::from)?
                .ok_or_else(|| VfsError::NotFound {
                    path: PathHint::some(dir.clone()),
                })?
                .0
        };

        let ino = self
            .core
            .meta_layer
            .lookup(parent_ino, &name)
            .await
            .map_err(VfsError::from)?
            .ok_or_else(|| VfsError::NotFound {
                path: PathHint::some(path.clone()),
            })?;

        let attr = self
            .core
            .meta_layer
            .stat(ino)
            .await
            .map_err(VfsError::from)?
            .ok_or_else(|| VfsError::NotFound {
                path: PathHint::some(path.clone()),
            })?;

        if attr.kind == FileType::Dir {
            return Err(VfsError::IsADirectory {
                path: PathHint::some(path.clone()),
            });
        }

        self.core
            .meta_layer
            .unlink(parent_ino, &name)
            .await
            .map_err(VfsError::from)?;
        self.state.modified.touch(parent_ino).await;
        self.state.modified.touch(ino).await;

        Ok(())
    }

    /// Remove an empty directory (root cannot be removed; non-empty dirs error out).
    pub async fn rmdir(&self, path: &str) -> Result<(), VfsError> {
        let path = Self::norm_path(path);
        if path == "/" {
            return Err(VfsError::PermissionDenied {
                path: PathHint::some(path),
            });
        }

        let (dir, name) = Self::split_dir_file(&path);

        let parent_ino = if &dir == "/" {
            self.core.root
        } else {
            self.core
                .meta_layer
                .lookup_path(&dir)
                .await
                .map_err(VfsError::from)?
                .ok_or_else(|| VfsError::NotFound {
                    path: PathHint::some(dir.clone()),
                })?
                .0
        };

        let ino = self
            .core
            .meta_layer
            .lookup(parent_ino, &name)
            .await
            .map_err(VfsError::from)?
            .ok_or_else(|| VfsError::NotFound {
                path: PathHint::some(path.clone()),
            })?;

        let attr = self
            .core
            .meta_layer
            .stat(ino)
            .await
            .map_err(VfsError::from)?
            .ok_or_else(|| VfsError::NotFound {
                path: PathHint::some(path.clone()),
            })?;

        if attr.kind != FileType::Dir {
            return Err(VfsError::NotADirectory {
                path: PathHint::some(path.clone()),
            });
        }

        let children = self
            .core
            .meta_layer
            .readdir(ino)
            .await
            .map_err(VfsError::from)?;
        if !children.is_empty() {
            return Err(VfsError::DirectoryNotEmpty {
                path: PathHint::some(path.clone()),
            });
        }

        self.core
            .meta_layer
            .rmdir(parent_ino, &name)
            .await
            .map_err(VfsError::from)?;
        self.state.modified.touch(parent_ino).await;
        self.state.modified.touch(ino).await;

        Ok(())
    }

    /// Rename files or directories.
    ///
    /// Implements POSIX rename semantics: if the destination exists, it will be replaced,
    /// subject to appropriate checks (e.g., file/directory type compatibility, non-empty directories).
    /// Parent directories are created as needed.
    pub async fn rename(&self, old: &str, new: &str) -> Result<(), VfsError> {
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
                .await
                .map_err(VfsError::from)?
                .ok_or_else(|| VfsError::NotFound {
                    path: PathHint::some(old_dir.clone()),
                })?
                .0
        };

        let src_ino = self
            .core
            .meta_layer
            .lookup(old_parent_ino, &old_name)
            .await
            .map_err(VfsError::from)?
            .ok_or_else(|| VfsError::NotFound {
                path: PathHint::some(old.clone()),
            })?;

        let src_attr = self
            .core
            .meta_layer
            .stat(src_ino)
            .await
            .map_err(VfsError::from)?
            .ok_or_else(|| VfsError::NotFound {
                path: PathHint::some(old.clone()),
            })?;

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
                    .await
                    .map_err(VfsError::from)?
                    .ok_or_else(|| VfsError::NotFound {
                        path: PathHint::some(new_dir.clone()),
                    })?
                    .0
            };

            if dest_kind == FileType::Dir {
                // source must be directory
                if src_attr.kind != FileType::Dir {
                    return Err(VfsError::NotADirectory {
                        path: PathHint::some(old.clone()),
                    });
                }

                // ensure destination dir is empty
                let children = self
                    .core
                    .meta_layer
                    .readdir(dest_ino)
                    .await
                    .map_err(VfsError::from)?;
                if !children.is_empty() {
                    return Err(VfsError::DirectoryNotEmpty {
                        path: PathHint::some(new.clone()),
                    });
                }

                // remove the empty destination directory
                self.core
                    .meta_layer
                    .rmdir(new_dir_ino, &new_name)
                    .await
                    .map_err(VfsError::from)?;
            } else {
                if src_attr.kind == FileType::Dir {
                    return Err(VfsError::NotADirectory {
                        path: PathHint::some(new.clone()),
                    });
                }
                // dest is a file or symlink: unlink it to allow replace
                self.core
                    .meta_layer
                    .unlink(new_dir_ino, &new_name)
                    .await
                    .map_err(VfsError::from)?;
            }
        }

        // Ensure destination parent exists (create as needed)
        let new_dir_ino = self.mkdir_p(&new_dir).await?;

        // Perform rename
        self.core
            .meta_layer
            .rename(old_parent_ino, &old_name, new_dir_ino, new_name)
            .await
            .map_err(VfsError::from)?;

        self.state.modified.touch(old_parent_ino).await;
        self.state.modified.touch(new_dir_ino).await;

        Ok(())
    }

    /// Truncate/extend file size (metadata only; holes are read as zeros).
    /// Shrinking does not eagerly reclaim block data.
    pub async fn truncate(&self, path: &str, size: u64) -> Result<(), VfsError> {
        let path = Self::norm_path(path);
        let (ino, _) = self
            .core
            .meta_layer
            .lookup_path(&path)
            .await
            .map_err(VfsError::from)?
            .ok_or_else(|| VfsError::NotFound {
                path: PathHint::some(path.clone()),
            })?;

        let fhs = self.state.handles.handles_for(ino);
        let mut guards = Vec::with_capacity(fhs.len());
        for fh in fhs {
            if let Some(handle) = self.state.handles.get(fh) {
                guards.push(handle.lock_write().await);
            }
        }

        self.state.writer.flush_if_exists(ino as u64).await;
        self.core
            .meta_layer
            .truncate(ino, size, self.core.layout.chunk_size)
            .await
            .map_err(VfsError::from)?;

        // POSIX semantic for `truncate`: `truncate` is immediately visible to old handles.
        self.state.reader.invalidate_all(ino as u64).await;
        self.state.writer.clear(ino as u64).await;

        let guard = self
            .lock_inode(ino)
            .or_insert_with(|| Inode::new(ino, size));
        guard.update_size(size);

        if let Some(mut attr) = self.state.handles.attr_for_inode(ino) {
            attr.size = size;
            self.state.handles.update_attr_for_inode(ino, &attr);
        }

        self.state.modified.touch(ino).await;
        drop(guards);
        Ok(())
    }

    pub async fn set_attr(
        &self,
        ino: i64,
        req: &SetAttrRequest,
        flags: SetAttrFlags,
    ) -> Result<FileAttr, VfsError> {
        if let Some(size) = req.size {
            self.core
                .meta_layer
                .truncate(ino, size, self.core.layout.chunk_size)
                .await
                .map_err(VfsError::from)?;
        }

        let mut filtered = *req;
        filtered.size = None;
        let attr = self
            .core
            .meta_layer
            .set_attr(ino, &filtered, flags)
            .await
            .map_err(VfsError::from)?;

        if let Some(size) = req.size
            && let Some(inode) = self.state.inodes.get(&ino)
        {
            inode.update_size(size);
        }

        self.state.modified.touch(ino).await;
        self.state.handles.update_attr_for_inode(ino, &attr);

        Ok(attr)
    }

    /// Read data by file handle and offset.
    #[tracing::instrument(level = "trace", skip(self), fields(fh, offset, len))]
    pub async fn read(&self, fh: u64, offset: u64, len: usize) -> Result<Vec<u8>, VfsError> {
        if len == 0 {
            return Ok(Vec::new());
        }

        let handle = self
            .state
            .handles
            .get(fh)
            .ok_or(VfsError::StaleNetworkFileHandle)?;
        if !handle.flags.read {
            return Err(VfsError::PermissionDenied {
                path: PathHint::none(),
            });
        }

        // Before reading, it is needed to flush all cached data.
        self.state.writer.flush_if_exists(handle.ino as u64).await;
        handle.read(offset, len).await.map_err(VfsError::from)
    }

    /// Write data by file handle and offset.
    #[tracing::instrument(level = "trace", skip(self, data), fields(fh, offset, len = data.len()))]
    pub async fn write(&self, fh: u64, offset: u64, data: &[u8]) -> Result<usize, VfsError> {
        if data.is_empty() {
            return Ok(0);
        }

        let handle = self
            .state
            .handles
            .get(fh)
            .ok_or(VfsError::StaleNetworkFileHandle)?;
        if !handle.flags.write {
            return Err(VfsError::PermissionDenied {
                path: PathHint::none(),
            });
        }

        let written = handle.write(offset, data).await?;

        let target_size = offset + written as u64;

        // POSIX semantic for `write`: it is unable to shorten the file.
        let extend_start = Instant::now();
        self.core
            .meta_layer
            .extend_file_size(handle.ino, target_size)
            .await?;
        if let Some((calls, avg_us, max_us)) =
            self.state.extend_stats.record(extend_start.elapsed())
        {
            tracing::info!(calls, avg_us, max_us, "VFS::extend_file_size stats");
        }
        self.state.modified.touch(handle.ino).await;
        Ok(written)
    }

    /// Allocate a per-file handle, returning the opaque fh id.
    pub async fn open(
        &self,
        ino: i64,
        attr: FileAttr,
        read: bool,
        write: bool,
    ) -> Result<u64, VfsError> {
        let mut latest_attr = attr;

        // Retrieve the latest attr for close-to-open semantics.
        match self.core.meta_layer.stat_fresh(ino).await {
            Ok(Some(fresh)) => {
                latest_attr = fresh;
            }
            Ok(None) => {}
            Err(err) => {
                tracing::warn!("open: stat_fresh failed for ino {}: {}", ino, err);
                return Err(VfsError::StaleNetworkFileHandle);
            }
        }

        let guard = self
            .lock_inode(ino)
            .or_insert_with(|| Inode::new(ino, latest_attr.size));
        if latest_attr.size > guard.file_size() {
            guard.update_size(latest_attr.size);
        }

        let inode = guard.clone();
        let handle = self
            .state
            .handles
            .allocate(ino, latest_attr, HandleFlags::new(read, write));

        let reader = self.state.reader.open_for_handle(inode.clone(), handle.fh);
        handle.reader(reader);
        if write {
            let writer = self.state.writer.ensure_file(inode.clone());
            handle.writer(writer);
        }
        Ok(handle.fh)
    }

    /// Release a previously allocated file handle.
    pub async fn close(&self, fh: u64) -> Result<(), VfsError> {
        // Note that we cannot hold the lock during the entire function, because `handle.flush()` is a I/O operation.
        let handle = self
            .state
            .handles
            .get(fh)
            .ok_or(VfsError::StaleNetworkFileHandle)?;

        if handle.flags.write {
            handle.flush().await.map_err(|_| VfsError::Other)?;
            self.update_mtime_ctime(handle.ino).await?;
        }

        // Prevent us from TOC-TOU (time of check to time of use) error.
        // If we release the handle and remove the inode directly, there is
        // a time windows between checking and releasing. It causes the inode and writer
        // to be deleted mistakenly.
        match self.lock_inode(handle.ino) {
            Entry::Occupied(entry) => {
                self.state.handles.release(fh);
                self.state.reader.close_for_handle(handle.ino as u64, fh);

                if !self.state.handles.has_write_handle(handle.ino) {
                    self.state.writer.release(handle.ino as u64);
                }

                if self.state.handles.has_no_handle(handle.ino) {
                    entry.remove();
                }
            }
            Entry::Vacant(_) => {
                // This is weird/impossible?
                // It means the inode was deleted while we held a handle to it.
                unreachable!("Try closing a file that has never been opened");
            }
        }

        Ok(())
    }

    /// Open a directory handle for reading. Returns the file handle ID.
    /// This pre-loads all directory entries and starts background batch prefetch for attributes.
    pub async fn opendir(&self, ino: i64) -> Result<u64, VfsError> {
        // Verify directory exists
        let attr = self
            .core
            .meta_layer
            .stat(ino)
            .await?
            .ok_or(VfsError::NotFound {
                path: PathHint::none(),
            })?;

        if attr.kind != FileType::Dir {
            return Err(VfsError::NotADirectory {
                path: PathHint::none(),
            });
        }

        // Load all directory entries
        let entries = self.core.meta_layer.readdir(ino).await?;

        // Start background batch prefetch task
        let (done_flag, prefetch_task) = self.core.meta_layer.spawn_batch_prefetch(ino, &entries);

        // Create handle with prefetch task
        let handle = DirHandle::with_prefetch_task(ino, entries, prefetch_task, done_flag);
        let fh = self.state.handles.allocate_dir(handle);

        Ok(fh)
    }

    /// Close a directory handle
    pub fn closedir(&self, fh: u64) -> Result<(), VfsError> {
        let handle = self
            .state
            .handles
            .release_dir(fh)
            .ok_or(VfsError::StaleNetworkFileHandle)?;

        tracing::info!(
            "release dir handle: fh={}, ino={}, entries={}",
            fh,
            handle.ino,
            handle.entries.len()
        );

        // Check if prefetch task is still running
        let is_done = handle.prefetch_done.load(Ordering::Acquire);
        if !is_done {
            tracing::debug!(
                "Dir handle fh={}, ino={} released while prefetch still running - task will be aborted on drop",
                fh,
                handle.ino
            );
        }
        // When handle is dropped (Arc refcount reaches 0), DirHandle::drop() will abort the task

        Ok(())
    }

    /// Read directory entries by handle with pagination
    pub fn readdir(&self, fh: u64, offset: u64) -> Option<Vec<DirEntry>> {
        let handle = self.state.handles.get_dir(fh)?;
        Some(handle.get_entries(offset))
    }

    /// Update cached information about a handle (e.g. last observed offset).
    pub(crate) fn touch_handle_offset(&self, fh: u64, offset: u64) -> Result<(), VfsError> {
        self.state
            .handles
            .get(fh)
            .map(|handle| handle.update_offset(offset))
            .ok_or(VfsError::StaleNetworkFileHandle)
    }

    /// List all open handles for an inode.
    pub(crate) fn handles_for(&self, ino: i64) -> Vec<u64> {
        self.state.handles.handles_for(ino)
    }

    pub(crate) fn handle_attr(&self, fh: u64) -> Option<FileAttr> {
        self.state.handles.attr_for(fh)
    }

    pub(crate) fn handle_attr_by_ino(&self, ino: i64) -> Option<FileAttr> {
        self.state.handles.attr_for_inode(ino)
    }

    /// Check whether a file has been modified since a given point in time.
    pub(crate) async fn modified_since(&self, ino: i64, since: Instant) -> bool {
        self.state.modified.modified_since(ino, since).await
    }

    /// Drop modification markers older than `ttl` to keep the tracker bounded.
    pub(crate) async fn cleanup_modified(&self, ttl: Duration) {
        self.state.modified.cleanup_older_than(ttl).await;
    }

    /// Get file lock information for a given inode and query.
    pub(crate) async fn get_plock_ino(
        &self,
        inode: i64,
        query: &FileLockQuery,
    ) -> Result<FileLockInfo, VfsError> {
        self.core
            .meta_layer
            .get_plock(inode, query)
            .await
            .map_err(VfsError::from)
    }

    /// Set file lock for a given inode.
    pub(crate) async fn set_plock_ino(
        &self,
        inode: i64,
        owner: i64,
        block: bool,
        lock_type: FileLockType,
        range: FileLockRange,
        pid: u32,
    ) -> Result<(), VfsError> {
        self.core
            .meta_layer
            .set_plock(inode, owner, block, lock_type, range, pid)
            .await
            .map_err(VfsError::from)
    }

    /// Get file lock information by path.
    pub async fn get_plock(
        &self,
        path: &str,
        query: &FileLockQuery,
    ) -> Result<FileLockInfo, VfsError> {
        let path = Self::norm_path(path);
        let (inode, _) = self
            .core
            .meta_layer
            .lookup_path(&path)
            .await
            .map_err(VfsError::from)?
            .ok_or_else(|| VfsError::NotFound {
                path: PathHint::some(path.clone()),
            })?;
        self.core
            .meta_layer
            .get_plock(inode, query)
            .await
            .map_err(VfsError::from)
    }

    /// Set file lock by path.
    pub async fn set_plock(
        &self,
        path: &str,
        owner: i64,
        block: bool,
        lock_type: FileLockType,
        range: FileLockRange,
        pid: u32,
    ) -> Result<(), VfsError> {
        let path = Self::norm_path(path);
        let (inode, _) = self
            .core
            .meta_layer
            .lookup_path(&path)
            .await
            .map_err(VfsError::from)?
            .ok_or_else(|| VfsError::NotFound {
                path: PathHint::some(path.clone()),
            })?;
        self.core
            .meta_layer
            .set_plock(inode, owner, block, lock_type, range, pid)
            .await
            .map_err(VfsError::from)
    }

    /// Update timestamps on flush/fsync for files that may have been modified via mmap.
    /// This is necessary because the kernel doesn't call write() for mmap writes.
    /// We only update if the file was opened for writing.
    pub(crate) async fn update_timestamps_on_flush(&self, ino: i64) -> Result<(), VfsError> {
        // Check if any handle for this inode was opened for writing
        let has_write_handle = self.state.handles.has_write_handle(ino);

        if has_write_handle {
            // File was opened for writing, update mtime/ctime to handle potential mmap writes
            self.update_mtime_ctime(ino).await?;
        }

        Ok(())
    }

    async fn ensure_inode_registered(&self, ino: i64) -> Result<Arc<Inode>, VfsError> {
        if let Some(inode) = self.state.inodes.get(&ino) {
            return Ok(inode.clone());
        }

        match self.lock_inode(ino) {
            Entry::Occupied(entry) => Ok(Arc::clone(entry.get())),
            Entry::Vacant(entry) => {
                let attr = self
                    .core
                    .meta_layer
                    .stat(ino)
                    .await?
                    .ok_or(VfsError::NotFound {
                        path: PathHint::none(),
                    })?;
                if attr.kind != FileType::File {
                    let err = match attr.kind {
                        FileType::Dir => VfsError::IsADirectory {
                            path: PathHint::none(),
                        },
                        _ => VfsError::InvalidInput,
                    };
                    return Err(err);
                }

                let inode = Inode::new(ino, attr.size);
                entry.insert(inode.clone());
                Ok(inode)
            }
        }
    }

    fn lock_inode(&self, ino: i64) -> Entry<'_, i64, Arc<Inode>> {
        self.state.inodes.entry(ino)
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
    use rand::rngs::StdRng;
    use rand::{Rng, RngCore, SeedableRng};
    use std::sync::Arc;
    use tokio::sync::Barrier;

    async fn open_file<S, M>(fs: &VFS<S, M>, path: &str, read: bool, write: bool) -> u64
    where
        S: BlockStore + Send + Sync + 'static,
        M: MetaStore + Send + Sync + 'static,
    {
        let attr = fs.stat(path).await.expect("stat");
        fs.open(attr.ino, attr, read, write).await.unwrap()
    }

    async fn write_path<S, M>(fs: &VFS<S, M>, path: &str, offset: u64, data: &[u8]) -> usize
    where
        S: BlockStore + Send + Sync + 'static,
        M: MetaStore + Send + Sync + 'static,
    {
        let fh = open_file(fs, path, false, true).await;
        let result = fs.write(fh, offset, data).await.expect("write");
        let _ = fs.close(fh).await;
        result
    }

    async fn read_path<S, M>(fs: &VFS<S, M>, path: &str, offset: u64, len: usize) -> Vec<u8>
    where
        S: BlockStore + Send + Sync + 'static,
        M: MetaStore + Send + Sync + 'static,
    {
        let fh = open_file(fs, path, true, false).await;
        let result = fs.read(fh, offset, len).await.expect("read");
        let _ = fs.close(fh).await;
        result
    }

    async fn readdir_path<S, M>(fs: &VFS<S, M>, path: &str) -> Vec<DirEntry>
    where
        S: BlockStore + Send + Sync + 'static,
        M: MetaStore + Send + Sync + 'static,
    {
        let attr = fs.stat(path).await.expect("stat");
        let fh = fs.opendir(attr.ino).await.expect("opendir");
        let mut offset = 0u64;
        let mut entries = Vec::new();
        loop {
            let batch = fs.readdir(fh, offset).unwrap_or_default();
            if batch.is_empty() {
                break;
            }
            offset += batch.len() as u64;
            entries.extend(batch);
        }
        let _ = fs.closedir(fh);
        entries
    }

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
        write_path(&fs, "/a/b/hello.txt", (layout.block_size / 2) as u64, &data).await;
        let (ino, _) = fs
            .core
            .meta_layer
            .lookup_path("/a/b/hello.txt")
            .await
            .unwrap()
            .unwrap();
        let inode = fs.ensure_inode_registered(ino).await.unwrap();
        let writer = fs.state.writer.ensure_file(inode);
        writer.flush().await.unwrap();
        let out = read_path(
            &fs,
            "/a/b/hello.txt",
            (layout.block_size / 2) as u64,
            data_len,
        )
        .await;
        assert_eq!(out, data);

        let entries = readdir_path(&fs, "/a/b").await;
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

    #[tokio::test]
    async fn test_fs_truncate_prunes_chunks_and_zero_fills() {
        let layout = ChunkLayout {
            chunk_size: 8 * 1024,
            block_size: 4 * 1024,
        };
        let store = InMemoryBlockStore::new();
        let meta_handle = create_meta_store_from_url("sqlite::memory:").await.unwrap();
        let meta_store = meta_handle.store();
        let fs = VFS::new(layout, store, meta_store).await.unwrap();

        fs.create_file("/t.bin").await.unwrap();

        let len = layout.chunk_size as usize + 2048;
        let mut data = vec![0u8; len];
        for (i, b) in data.iter_mut().enumerate() {
            *b = (i % 251) as u8;
        }
        write_path(&fs, "/t.bin", 0, &data).await;

        fs.truncate("/t.bin", 1024).await.unwrap();
        let head = read_path(&fs, "/t.bin", 0, 4096).await;
        assert_eq!(head.len(), 1024);
        assert_eq!(head, data[..1024].to_vec());

        let new_size = layout.chunk_size + 4096;
        fs.truncate("/t.bin", new_size).await.unwrap();
        let st = fs.stat("/t.bin").await.unwrap();
        assert_eq!(st.size, new_size);

        let hole = read_path(&fs, "/t.bin", layout.chunk_size + 512, 1024).await;
        assert_eq!(hole, vec![0u8; 1024]);
    }

    #[tokio::test]
    async fn test_fs_close_releases_writer_and_inode() {
        let layout = ChunkLayout {
            chunk_size: 8 * 1024,
            block_size: 4 * 1024,
        };
        let store = InMemoryBlockStore::new();
        let meta_handle = create_meta_store_from_url("sqlite::memory:").await.unwrap();
        let meta_store = meta_handle.store();
        let fs = VFS::new(layout, store, meta_store).await.unwrap();

        fs.create_file("/close.bin").await.unwrap();
        let attr = fs.stat("/close.bin").await.unwrap();
        let fh = fs.open(attr.ino, attr.clone(), false, true).await.unwrap();
        let data = vec![1u8; 2048];
        fs.write(fh, 0, &data).await.unwrap();
        fs.close(fh).await.unwrap();

        assert!(!fs.state.writer.has_file(attr.ino as u64));
        assert!(!fs.state.inodes.contains_key(&attr.ino));
    }

    #[tokio::test]
    async fn test_fs_truncate_extend_does_not_return_stale_reader_cache() {
        let layout = ChunkLayout {
            chunk_size: 8 * 1024,
            block_size: 4 * 1024,
        };
        let store = InMemoryBlockStore::new();
        let meta_handle = create_meta_store_from_url("sqlite::memory:").await.unwrap();
        let meta_store = meta_handle.store();
        let fs = VFS::new(layout, store, meta_store).await.unwrap();

        fs.create_file("/stale_trunc.bin").await.unwrap();

        let len = layout.chunk_size as usize + 2048;
        let mut data = vec![0u8; len];
        for (i, b) in data.iter_mut().enumerate() {
            *b = (i % 251) as u8;
        }
        write_path(&fs, "/stale_trunc.bin", 0, &data).await;

        let attr = fs.stat("/stale_trunc.bin").await.unwrap();
        let fh = fs.open(attr.ino, attr.clone(), true, false).await.unwrap();

        let offset = layout.block_size as u64;
        let probe_len = 1024usize;
        let original = fs.read(fh, offset, probe_len).await.unwrap();
        assert_eq!(
            original,
            data[offset as usize..offset as usize + probe_len].to_vec()
        );

        fs.truncate("/stale_trunc.bin", 1024).await.unwrap();
        fs.truncate("/stale_trunc.bin", len as u64).await.unwrap();

        let after = fs.read(fh, offset, probe_len).await.unwrap();
        assert_eq!(after, vec![0u8; probe_len]);

        fs.close(fh).await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_fs_parallel_writes_to_distinct_files() {
        let layout = ChunkLayout {
            chunk_size: 8 * 1024,
            block_size: 4 * 1024,
        };
        let store = InMemoryBlockStore::new();
        let meta_handle = create_meta_store_from_url("sqlite::memory:").await.unwrap();
        let meta_store = meta_handle.store();
        let fs = Arc::new(VFS::new(layout, store, meta_store).await.unwrap());

        fs.mkdir_p("/data").await.unwrap();

        let file_count = 4usize;
        let barrier = Arc::new(Barrier::new(file_count + 1));
        let mut handles = Vec::new();

        for i in 0..file_count {
            let path = format!("/data/f{i}.bin");
            fs.create_file(&path).await.unwrap();

            let len = match i {
                0 => 1024,
                1 => layout.block_size as usize,
                2 => layout.block_size as usize + 512,
                _ => layout.chunk_size as usize + 512,
            };
            let mut data = vec![0u8; len];
            for (idx, b) in data.iter_mut().enumerate() {
                *b = (i as u8).wrapping_add(idx as u8);
            }

            let fs_clone = fs.clone();
            let barrier = barrier.clone();
            handles.push(tokio::spawn(async move {
                barrier.wait().await;
                write_path(&fs_clone, &path, 0, &data).await;
                (path, data)
            }));
        }

        barrier.wait().await;

        let mut results = Vec::new();
        for handle in handles {
            results.push(handle.await.unwrap());
        }

        for (path, _) in results.iter() {
            let (ino, _) = fs.core.meta_layer.lookup_path(path).await.unwrap().unwrap();
            let inode = fs.ensure_inode_registered(ino).await.unwrap();
            let writer = fs.state.writer.ensure_file(inode);
            writer.flush().await.unwrap();
        }

        for (path, data) in results {
            let out = read_path(&fs, &path, 0, data.len()).await;
            assert_eq!(out, data);
        }
    }

    /// The test will take approximately 10 seconds to complete.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_fs_fuzz_parallel_read_write() {
        let layout = ChunkLayout {
            chunk_size: 16 * 1024,
            block_size: 4 * 1024,
        };
        let store = InMemoryBlockStore::new();
        let meta_handle = create_meta_store_from_url("sqlite::memory:").await.unwrap();
        let meta_store = meta_handle.store();
        let fs = Arc::new(VFS::new(layout, store, meta_store).await.unwrap());

        fs.mkdir_p("/fuzz").await.unwrap();

        let file_count = 4usize;
        let mut paths = Vec::with_capacity(file_count);
        let mut states = Vec::with_capacity(file_count);

        for i in 0..file_count {
            let path = format!("/fuzz/f{i}.bin");
            fs.create_file(&path).await.unwrap();
            paths.push(path);
            states.push(Arc::new(tokio::sync::Mutex::new(Vec::<u8>::new())));
        }

        let task_count = 4usize;
        let iterations = 5000usize;
        let max_write = 4096usize;

        let mut handles = Vec::with_capacity(task_count);
        for t in 0..task_count {
            let fs = fs.clone();
            let paths = paths.clone();
            let states = states.clone();
            let mut rng = StdRng::seed_from_u64(0x5EED_u64 + t as u64);
            handles.push(tokio::spawn(async move {
                for _ in 0..iterations {
                    let file_idx = rng.random_range(0..file_count);
                    let path = paths[file_idx].clone();
                    let state = states[file_idx].clone();

                    if rng.random_range(0..100) < 60 {
                        let mut guard = state.lock().await;
                        let cur_len = guard.len();
                        let max_offset = cur_len + layout.block_size as usize;
                        let offset = rng.random_range(0..=max_offset);
                        let len = rng.random_range(1..=max_write);
                        let mut data = vec![0u8; len];
                        rng.fill_bytes(&mut data);

                        write_path(&fs, &path, offset as u64, &data).await;

                        let end = offset + len;
                        if guard.len() < end {
                            guard.resize(end, 0);
                        }
                        guard[offset..end].copy_from_slice(&data);
                    } else {
                        let guard = state.lock().await;
                        let cur_len = guard.len();
                        if cur_len == 0 {
                            let out = read_path(&fs, &path, 0, 0).await;
                            assert!(out.is_empty());
                            continue;
                        }
                        let offset = rng.random_range(0..cur_len);
                        let len = rng.random_range(1..=std::cmp::min(cur_len - offset, max_write));
                        let expected = guard[offset..offset + len].to_vec();
                        let out = read_path(&fs, &path, offset as u64, len).await;
                        assert_eq!(out, expected);
                    }
                }
            }));
        }

        for handle in handles {
            handle.await.unwrap();
        }

        for (path, state) in paths.iter().zip(states.iter()) {
            let path = path.clone();
            let state = state.clone();
            let guard = state.lock().await;
            let expected = guard.clone();
            let out = read_path(&fs, &path, 0, expected.len()).await;
            assert_eq!(out, expected);
        }
    }
}
