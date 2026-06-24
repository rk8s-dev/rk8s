//! FUSE/SDK-friendly VFS with path-based metadata ops and handle-based IO.

use crate::chunk::store::BlockStore;
use crate::chunk::{BlockGcConfig, ChunkLayout, CompactionWorker, CompactionWorkerConfig};
use crate::meta::MetaLayer;
use crate::meta::client::MetaClient;
use crate::meta::config::CompactConfig;
use crate::meta::config::MetaClientConfig;
use crate::meta::file_lock::{FileLockInfo, FileLockQuery, FileLockRange, FileLockType};
use crate::meta::store::{AclRule, MetaStore, SetAttrFlags, SetAttrRequest, StatFsSnapshot};
use dashmap::{DashMap, Entry};
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

// Re-export types from meta::store for convenience
pub use crate::meta::store::{DirEntry, FileAttr, FileType};

/// Rename operation flags (similar to Linux renameat2 flags)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RenameFlags {
    /// Don't overwrite the destination if it exists (RENAME_NOREPLACE)
    pub noreplace: bool,
    /// Atomically exchange the source and destination (RENAME_EXCHANGE)
    pub exchange: bool,
    /// Remove the destination if it's a whiteout (RENAME_WHITEOUT)
    pub whiteout: bool,
}

/// Configuration for VFS background tasks
#[derive(Debug, Clone)]
pub struct VfsBackgroundConfig {
    pub compaction: CompactionWorkerConfig,
    pub gc: BlockGcConfig,
    pub compact_config: CompactConfig,
    pub enabled: bool,
}

impl VfsBackgroundConfig {
    pub fn from_compact_config(
        layout: &ChunkLayout,
        compact_config: CompactConfig,
        enabled: bool,
    ) -> Self {
        let compaction = CompactionWorkerConfig {
            scan_interval: compact_config.interval,
            max_chunks_per_run: compact_config.max_chunks_per_run,
            enabled,
        };

        let gc = BlockGcConfig {
            block_size: layout.block_size as u64,
            interval: compact_config.interval,
            ..Default::default()
        };

        Self {
            compaction,
            gc,
            compact_config,
            enabled,
        }
    }
}

impl Default for VfsBackgroundConfig {
    fn default() -> Self {
        Self {
            compaction: CompactionWorkerConfig::default(),
            gc: BlockGcConfig::default(),
            compact_config: CompactConfig::default(),
            enabled: true,
        }
    }
}

struct VfsBackgroundTasks {
    compaction_handle: tokio::task::JoinHandle<()>,
    gc_handle: tokio::task::JoinHandle<()>,
}

use crate::vfs::Inode;
use crate::vfs::backend::Backend;
use crate::vfs::config::VFSConfig;
use crate::vfs::error::{PathHint, VfsError};
use crate::vfs::handles::{DirHandle, FileHandle, HandleFlags};
use crate::vfs::io::{DataReader, DataWriter};

struct HandleRegistry<B, M>
where
    B: BlockStore + Send + Sync + 'static,
    M: MetaLayer + Send + Sync + 'static,
{
    handles: DashMap<u64, Arc<FileHandle<B, M>>>,
    inode_handles: DashMap<i64, Vec<u64>>,
    dir_handles: DashMap<u64, Arc<DirHandle>>,
    next_fh: AtomicU64,
}

impl<B, M> HandleRegistry<B, M>
where
    B: BlockStore + Send + Sync + 'static,
    M: MetaLayer + Send + Sync + 'static,
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
    M: MetaLayer + Send + Sync + 'static,
{
    handles: HandleRegistry<S, M>,
    inodes: DashMap<i64, Arc<Inode>>,
    reader: Arc<DataReader<S, M>>,
    writer: Arc<DataWriter<S, M>>,
    modified: ModifiedTracker,
}

impl<S, M> VfsState<S, M>
where
    S: BlockStore + Send + Sync + 'static,
    M: MetaLayer + Send + Sync + 'static,
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
        }
    }
}

#[allow(dead_code)]
pub(crate) struct VfsCore<S, M>
where
    S: BlockStore + Send + Sync + 'static,
    M: MetaLayer + Send + Sync + 'static,
{
    layout: ChunkLayout,
    pub(crate) backend: Arc<Backend<S, M>>,
    pub(crate) meta_layer: Arc<M>,
    root: i64,
}

impl<S, M> VfsCore<S, M>
where
    S: BlockStore + Send + Sync + 'static,
    M: MetaLayer + Send + Sync + 'static,
{
    pub(crate) fn new(
        layout: ChunkLayout,
        backend: Arc<Backend<S, M>>,
        meta_layer: Arc<M>,
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

#[allow(unused)]
#[allow(clippy::upper_case_acronyms)]
pub struct VFS<S, M>
where
    S: BlockStore + Send + Sync + 'static,
    M: MetaLayer + Send + Sync + 'static,
{
    core: Arc<VfsCore<S, M>>,
    state: Arc<VfsState<S, M>>,
    /// Background tasks (compaction and gc) - only present when enabled
    #[allow(dead_code)]
    background_tasks: Option<VfsBackgroundTasks>,
}

impl<S, M> Clone for VFS<S, M>
where
    S: BlockStore + Send + Sync + 'static,
    M: MetaLayer + Send + Sync + 'static,
{
    fn clone(&self) -> Self {
        Self {
            core: Arc::clone(&self.core),
            state: Arc::clone(&self.state),
            // Note: background tasks are not cloned as they should be unique per VFS instance
            background_tasks: None,
        }
    }
}

impl<S, R> VFS<S, MetaClient<R>>
where
    S: BlockStore + Send + Sync + 'static,
    R: MetaStore + Send + Sync + 'static,
{
    pub async fn new(layout: ChunkLayout, store: S, meta: R) -> Result<Self, VfsError> {
        Self::with_meta_client_config(layout, store, meta, MetaClientConfig::default()).await
    }

    pub(crate) async fn with_meta_client_config(
        layout: ChunkLayout,
        store: S,
        meta: R,
        config: MetaClientConfig,
    ) -> Result<Self, VfsError> {
        let store = Arc::new(store);
        let meta = Arc::new(meta);

        let ttl = config.effective_ttl();

        let meta_client = MetaClient::with_options(
            Arc::clone(&meta),
            config.capacity.clone(),
            ttl,
            config.options.clone(),
        );

        meta_client.initialize().await.map_err(VfsError::from)?;

        Self::with_meta_layer_with_compact_config(layout, store, meta_client, config.compact)
    }
}

impl<S, R> VFS<S, MetaClient<R>>
where
    S: BlockStore + Send + Sync + 'static,
    R: MetaStore + Send + Sync + ?Sized + 'static,
{
    pub(crate) fn with_meta_layer_with_compact_config(
        layout: ChunkLayout,
        store: Arc<S>,
        meta_layer: Arc<MetaClient<R>>,
        compact_config: CompactConfig,
    ) -> Result<Self, VfsError> {
        let enabled = !meta_layer.options().no_background_jobs;
        let bg_config = VfsBackgroundConfig::from_compact_config(&layout, compact_config, enabled);
        let background_tasks =
            Self::start_background_tasks(&meta_layer, Arc::clone(&store), layout, bg_config);

        Self::from_components_with_background(
            VFSConfig::new(layout),
            store,
            meta_layer,
            background_tasks,
        )
    }

    pub(crate) fn with_meta_layer_with_default_background(
        layout: ChunkLayout,
        store: Arc<S>,
        meta_layer: Arc<MetaClient<R>>,
    ) -> Result<Self, VfsError> {
        Self::with_meta_layer_with_compact_config(
            layout,
            store,
            meta_layer,
            CompactConfig::default(),
        )
    }

    /// Start background compaction and gc tasks
    fn start_background_tasks(
        meta_client: &Arc<MetaClient<R>>,
        block_store: Arc<S>,
        layout: ChunkLayout,
        config: VfsBackgroundConfig,
    ) -> Option<VfsBackgroundTasks> {
        if !config.enabled {
            return None;
        }

        let meta_store = meta_client.store();
        let is_database_store = meta_store.name() == "database";

        let mut worker = CompactionWorker::with_config(
            meta_store,
            block_store,
            layout,
            config.compact_config.clone(),
            config.compact_config.lock_ttl.clone(),
        );

        if is_database_store {
            let client = Arc::clone(meta_client);
            worker = worker.with_compaction_hook(Arc::new(move |chunk_id| {
                let client = Arc::clone(&client);
                tokio::spawn(async move {
                    client.invalidate_chunk_slices(chunk_id).await;
                });
            }));
        }
        let (compaction_handle, gc_handle) = worker.start(config.compaction, config.gc);

        Some(VfsBackgroundTasks {
            compaction_handle,
            gc_handle,
        })
    }
}

#[allow(dead_code)]
impl<S, M> VFS<S, M>
where
    S: BlockStore + Send + Sync + 'static,
    M: MetaLayer + Send + Sync + 'static,
{
    fn from_components_with_background(
        config: VFSConfig,
        store: Arc<S>,
        meta_layer: Arc<M>,
        background_tasks: Option<VfsBackgroundTasks>,
    ) -> Result<Self, VfsError> {
        let layout = config.write.layout;
        let root_ino = meta_layer.root_ino();
        let backend = Arc::new(Backend::new(store.clone(), meta_layer.clone()));
        let core = Arc::new(VfsCore::new(layout, backend.clone(), meta_layer, root_ino));
        let config = Arc::new(config);
        let state = Arc::new(VfsState::new(config, backend));

        Ok(Self {
            core,
            state,
            background_tasks,
        })
    }

    pub(crate) fn root_ino(&self) -> i64 {
        self.core.root
    }

    pub(crate) fn meta_layer(&self) -> &M {
        self.core.meta_layer.as_ref()
    }

    pub(crate) fn meta_layer_arc(&self) -> Arc<M> {
        Arc::clone(&self.core.meta_layer)
    }

    fn file_handle(&self, fh: u64) -> Option<Arc<FileHandle<S, M>>> {
        self.state.handles.get(fh)
    }

    fn file_handle_required(&self, fh: u64) -> Result<Arc<FileHandle<S, M>>, VfsError> {
        self.file_handle(fh).ok_or(VfsError::StaleNetworkFileHandle)
    }

    fn file_handles_for_inode(&self, ino: i64) -> Vec<Arc<FileHandle<S, M>>> {
        self.state
            .handles
            .handles_for(ino)
            .into_iter()
            .filter_map(|fh| self.file_handle(fh))
            .collect()
    }

    fn dir_handle(&self, fh: u64) -> Option<Arc<DirHandle>> {
        self.state.handles.get_dir(fh)
    }

    fn release_dir_handle_required(&self, fh: u64) -> Result<Arc<DirHandle>, VfsError> {
        self.state
            .handles
            .release_dir(fh)
            .ok_or(VfsError::StaleNetworkFileHandle)
    }

    pub(crate) fn inode_size_cached(&self, ino: i64) -> Option<u64> {
        self.state.inodes.get(&ino).map(|inode| inode.file_size())
    }

    pub(crate) async fn inode_size(&self, ino: i64) -> Result<u64, VfsError> {
        if let Some(size) = self.inode_size_cached(ino) {
            return Ok(size);
        }

        let attr = self.meta_stat_required(ino, PathHint::none()).await?;
        Ok(attr.size)
    }

    /// get the node's parent inode.
    pub async fn parent_of(&self, ino: i64) -> Option<i64> {
        self.meta_get_dir_parent(ino).await.ok().flatten()
    }

    /// get the node's fullpath.
    pub async fn path_of(&self, ino: i64) -> Option<String> {
        self.meta_get_paths(ino)
            .await
            .ok()
            .and_then(|paths| paths.into_iter().next())
    }

    /// get the node's child inode by name.
    pub(crate) async fn child_of(&self, parent: i64, name: &str) -> Option<i64> {
        self.meta_lookup(parent, name).await.ok().flatten()
    }

    #[tracing::instrument(level = "trace", skip(self), fields(ino))]
    pub(crate) async fn stat_ino(&self, ino: i64) -> Option<FileAttr> {
        let mut attr = self.meta_stat(ino).await.ok().flatten()?;

        // close-to-open semantics: if there is a local state, it should be considered as the newest state.
        if let Some(size) = self.inode_size_cached(ino) {
            attr.size = size;
        }

        Some(attr)
    }

    /// Returns the current time as nanoseconds since UNIX_EPOCH.
    fn current_timestamp_nanos() -> Result<i64, VfsError> {
        use std::time::{SystemTime, UNIX_EPOCH};
        Ok(SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| VfsError::Other)?
            .as_nanos() as i64)
    }

    /// Update atime (access time) for an inode to current time
    pub(crate) async fn update_atime(&self, ino: i64) -> Result<(), VfsError> {
        let now = Self::current_timestamp_nanos()?;

        let req = SetAttrRequest {
            atime: Some(now),
            ..Default::default()
        };

        self.meta_set_attr(ino, &req, SetAttrFlags::empty()).await?;

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
        let now = Self::current_timestamp_nanos()?;

        let req = SetAttrRequest {
            mtime: Some(now),
            ctime: Some(now),
            ..Default::default()
        };

        self.meta_set_attr(ino, &req, SetAttrFlags::empty()).await?;

        // Update handle cache if exists
        if let Some(mut attr) = self.state.handles.attr_for_inode(ino) {
            attr.mtime = now;
            attr.ctime = now;
            self.state.handles.update_attr_for_inode(ino, &attr);
        }

        Ok(())
    }

    /// List directory entries by inode
    #[tracing::instrument(level = "trace", skip(self), fields(ino))]
    pub(crate) async fn readdir_ino(&self, ino: i64) -> Option<Vec<DirEntry>> {
        let meta_entries = self.meta_readdir(ino).await.ok()?;

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
    #[tracing::instrument(level = "trace", skip(self), fields(path))]
    pub async fn mkdir_p(&self, path: &str) -> Result<i64, VfsError> {
        let path = Self::norm_path(path);
        if &path == "/" {
            return Ok(self.core.root);
        }
        if let Some((ino, _attr)) = self.meta_lookup_path(&path).await? {
            return Ok(ino);
        }
        let mut cur_ino = self.core.root;
        for part in path.trim_start_matches('/').split('/') {
            if part.is_empty() {
                continue;
            }
            let child = self.meta_lookup(cur_ino, part).await?;
            match child {
                Some(ino) => {
                    let attr = self
                        .meta_stat_required(ino, PathHint::some(path.as_str()))
                        .await?;
                    if attr.kind != FileType::Dir {
                        return Err(VfsError::NotADirectory {
                            path: PathHint::some(path.as_str()),
                        });
                    }
                    cur_ino = ino;
                }
                None => {
                    let ino = self.meta_mkdir(cur_ino, part.to_string()).await?;
                    self.state.modified.touch(cur_ino).await;
                    self.state.modified.touch(ino).await;
                    cur_ino = ino;
                }
            }
        }
        Ok(cur_ino)
    }

    /// Create a single directory (non-recursive).
    ///
    /// - Parent directory must exist.
    /// - If the target already exists as a directory, returns its inode.
    /// - If the target exists as a non-directory, returns `AlreadyExists`.
    /// - If parent does not exist, returns `NotFound`.
    pub async fn mkdir_err(&self, path: &str) -> Result<i64, VfsError> {
        let path = Self::norm_path(path);
        if path == "/" {
            return Ok(self.core.root);
        }

        let (dir, name) = Self::split_dir_file(&path);
        if name.is_empty() {
            return Err(VfsError::InvalidFilename);
        }

        let parent_ino = self.resolve_parent_inode(&dir).await?;

        // Check if target already exists
        if let Some(ino) = self.meta_lookup(parent_ino, &name).await? {
            let attr = self
                .meta_stat_required(ino, PathHint::some(path.as_str()))
                .await?;
            if attr.kind == FileType::Dir {
                return Ok(ino);
            }

            return Err(VfsError::AlreadyExists {
                path: PathHint::some(path.as_str()),
            });
        }

        // Create the directory
        let ino = self.meta_mkdir(parent_ino, name).await?;

        self.state.modified.touch(parent_ino).await;
        self.state.modified.touch(ino).await;

        Ok(ino)
    }

    /// Create a regular file in an existing parent directory (std-like behavior).
    ///
    /// - Does not create parent directories.
    /// - If the target exists and `create_new` is true, returns `AlreadyExists`.
    /// - If the target exists as a directory, returns `IsADirectory`.
    pub async fn create_file_in_existing_dir_err(
        &self,
        path: &str,
        create_new: bool,
    ) -> Result<i64, VfsError> {
        let path = Self::norm_path(path);
        if path == "/" {
            return Err(VfsError::IsADirectory { path: path.into() });
        }

        let (dir, name) = Self::split_dir_file(&path);
        if name.is_empty() {
            return Err(VfsError::InvalidFilename);
        }

        let parent_ino = self.resolve_parent_inode(&dir).await?;

        if let Some(existing) = self.meta_lookup(parent_ino, &name).await? {
            let attr = self
                .meta_stat_required(existing, PathHint::some(path.as_str()))
                .await?;
            if attr.kind == FileType::Dir {
                return Err(VfsError::IsADirectory {
                    path: PathHint::some(path.as_str()),
                });
            }
            if create_new {
                return Err(VfsError::AlreadyExists {
                    path: PathHint::some(path.as_str()),
                });
            }
            return Ok(existing);
        }

        let ino = self.meta_create_file(parent_ino, name).await?;
        self.state.modified.touch(parent_ino).await;
        self.state.modified.touch(ino).await;
        Ok(ino)
    }

    /// Create a regular file (running `mkdir_p` on its parent if needed).
    /// - If a directory with the same name exists, returns "is a directory".
    /// - If the file already exists, returns its inode instead of creating a new one.
    #[tracing::instrument(level = "trace", skip(self), fields(path))]
    pub async fn create_file(&self, path: &str) -> Result<i64, VfsError> {
        let path = Self::norm_path(path);
        let (dir, name) = Self::split_dir_file(&path);
        let dir_ino = self.mkdir_p(&dir).await?;

        // check the file exists and then return.
        if let Some(ino) = self.meta_lookup(dir_ino, &name).await? {
            let attr = self
                .meta_stat_required(ino, PathHint::some(path.as_str()))
                .await?;
            return if attr.kind == FileType::Dir {
                Err(VfsError::IsADirectory {
                    path: PathHint::some(path.as_str()),
                })
            } else {
                Ok(ino)
            };
        }

        let ino = self.meta_create_file(dir_ino, name.clone()).await?;
        self.state.modified.touch(dir_ino).await;
        self.state.modified.touch(ino).await;
        Ok(ino)
    }

    /// Create a hard link at `link_path` that references `existing_path`.
    #[tracing::instrument(level = "trace", skip(self), fields(existing_path, link_path))]
    pub async fn link(&self, existing_path: &str, link_path: &str) -> Result<FileAttr, VfsError> {
        let existing_path = Self::norm_path(existing_path);
        let link_path = Self::norm_path(link_path);

        if existing_path == "/" {
            return Err(VfsError::IsADirectory {
                path: PathHint::some(existing_path.as_str()),
            });
        }
        if link_path == "/" {
            return Err(VfsError::InvalidFilename);
        }

        let (src_ino, src_kind) = self.meta_lookup_path_required(&existing_path).await?;

        if src_kind == FileType::Dir {
            return Err(VfsError::IsADirectory {
                path: PathHint::some(existing_path.as_str()),
            });
        }

        let (parent_path, name) = Self::split_dir_file(&link_path);
        if name.is_empty() {
            return Err(VfsError::InvalidFilename);
        }

        let parent_ino = self.resolve_parent_inode(&parent_path).await?;

        let parent_attr = self
            .meta_stat_required(parent_ino, PathHint::some(parent_path.as_str()))
            .await?;
        if parent_attr.kind != FileType::Dir {
            return Err(VfsError::NotADirectory {
                path: PathHint::some(parent_path.as_str()),
            });
        }

        if self.meta_lookup(parent_ino, &name).await?.is_some() {
            return Err(VfsError::AlreadyExists {
                path: PathHint::some(link_path.as_str()),
            });
        }

        let attr = self.meta_link(src_ino, parent_ino, &name).await?;

        self.state.modified.touch(parent_ino).await;
        self.state.modified.touch(src_ino).await;

        Ok(attr)
    }

    /// Create a symbolic link at `link_path` pointing to `target`.
    #[tracing::instrument(level = "trace", skip(self), fields(link_path, target))]
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

        let parent_ino = self.resolve_parent_inode(&dir).await?;

        let parent_attr = self
            .meta_stat_required(parent_ino, PathHint::some(dir.as_str()))
            .await?;
        if parent_attr.kind != FileType::Dir {
            return Err(VfsError::NotADirectory {
                path: PathHint::some(dir.as_str()),
            });
        }

        if self.meta_lookup(parent_ino, &name).await?.is_some() {
            return Err(VfsError::AlreadyExists {
                path: PathHint::some(link_path.as_str()),
            });
        }

        let (ino, attr) = self.meta_symlink(parent_ino, &name, target).await?;

        self.state.modified.touch(parent_ino).await;
        self.state.modified.touch(ino).await;

        Ok((ino, attr))
    }

    /// Fetch a file's attributes (kind/size come from the metadata layer).
    #[tracing::instrument(level = "trace", skip(self), fields(path))]
    pub async fn stat(&self, path: &str) -> Result<FileAttr, VfsError> {
        let path = Self::norm_path(path);

        let (ino, _) = self.meta_lookup_path_required(&path).await?;

        let mut meta_attr = self
            .meta_stat_required(ino, PathHint::some(path.as_str()))
            .await?;

        // close-to-open semantics: if there is a local state, it should be considered as the newest state.
        if let Some(size) = self.inode_size_cached(ino) {
            meta_attr.size = size;
        }

        Ok(meta_attr)
    }

    /// Fetch a file's attributes (kind/size come from the MetaStore), following symlinks.
    pub async fn stat_follow_err(&self, path: &str) -> Result<FileAttr, VfsError> {
        self.stat(path).await
    }

    /// Read a symlink target by inode.
    #[tracing::instrument(level = "trace", skip(self), fields(ino))]
    pub(crate) async fn readlink_ino(&self, ino: i64) -> Result<String, VfsError> {
        let attr = self.meta_stat_required(ino, PathHint::none()).await?;
        if attr.kind != FileType::Symlink {
            return Err(VfsError::InvalidInput);
        }

        self.meta_read_symlink(ino).await
    }

    /// Read a symlink target by path.
    #[tracing::instrument(level = "trace", skip(self), fields(path))]
    pub async fn readlink(&self, path: &str) -> Result<String, VfsError> {
        let path = Self::norm_path(path);

        let (ino, kind) = self.meta_lookup_path_required(&path).await?;
        if kind != FileType::Symlink {
            return Err(VfsError::InvalidInput);
        }

        self.readlink_ino(ino).await
    }

    /// Check whether a path exists.
    pub async fn exists(&self, path: &str) -> bool {
        let path = Self::norm_path(path);
        matches!(self.meta_lookup_path(&path).await, Ok(Some(_)))
    }

    /// Remove a regular file or symlink (directories are not supported here).
    #[tracing::instrument(level = "trace", skip(self), fields(path))]
    pub async fn unlink(&self, path: &str) -> Result<(), VfsError> {
        let path = Self::norm_path(path);
        let (dir, name) = Self::split_dir_file(&path);

        let parent_ino = self.resolve_parent_inode(&dir).await?;

        let ino = self
            .meta_lookup_required(parent_ino, &name, PathHint::some(path.as_str()))
            .await?;

        let attr = self
            .meta_stat_required(ino, PathHint::some(path.as_str()))
            .await?;

        if attr.kind == FileType::Dir {
            return Err(VfsError::IsADirectory {
                path: PathHint::some(path.as_str()),
            });
        }

        self.meta_unlink(parent_ino, &name).await?;
        self.state.modified.touch(parent_ino).await;
        self.state.modified.touch(ino).await;

        Ok(())
    }

    /// Remove an empty directory (root cannot be removed; non-empty dirs error out).
    #[tracing::instrument(level = "trace", skip(self), fields(path))]
    pub async fn rmdir(&self, path: &str) -> Result<(), VfsError> {
        let path = Self::norm_path(path);
        if path == "/" {
            return Err(VfsError::PermissionDenied {
                path: PathHint::some(path),
            });
        }

        let (dir, name) = Self::split_dir_file(&path);

        let parent_ino = self.resolve_parent_inode(&dir).await?;

        let ino = self
            .meta_lookup_required(parent_ino, &name, PathHint::some(path.as_str()))
            .await?;

        let attr = self
            .meta_stat_required(ino, PathHint::some(path.as_str()))
            .await?;

        if attr.kind != FileType::Dir {
            return Err(VfsError::NotADirectory {
                path: PathHint::some(path.as_str()),
            });
        }

        let children = self.meta_readdir(ino).await?;
        if !children.is_empty() {
            return Err(VfsError::DirectoryNotEmpty {
                path: PathHint::some(path.as_str()),
            });
        }

        self.meta_rmdir(parent_ino, &name).await?;
        self.state.modified.touch(parent_ino).await;
        self.state.modified.touch(ino).await;

        Ok(())
    }

    /// Rename files or directories.
    ///
    /// Implements POSIX rename semantics: if the destination exists, it will be replaced,
    /// subject to appropriate checks (e.g., file/directory type compatibility, non-empty directories).
    /// Parent directories are created as needed.
    /// Check if renaming 'src_path' to 'dst_path' would create a circular reference.
    /// This prevents moving a directory into its own subdirectory.
    ///
    /// Note: Current implementation is limited because FileAttr doesn't expose parent_ino.
    /// A complete solution would require either:
    /// 1. Adding parent_ino to FileAttr
    /// 2. Walking up the directory tree using path-based lookups
    /// 3. Maintaining a separate parent tracking structure
    async fn is_circular_rename(
        &self,
        src_ino: i64,
        src_attr: &FileAttr,
        new_parent_ino: i64,
    ) -> Result<bool, VfsError> {
        // Only directories can create circular references
        if src_attr.kind != FileType::Dir {
            return Ok(false);
        }

        // Direct check: moving directory into itself
        if src_ino == new_parent_ino {
            return Ok(true);
        }

        // If moving to root, no circular reference possible
        if new_parent_ino == self.core.root {
            return Ok(false);
        }

        // Without parent tracking in metadata, we cannot reliably walk up the tree
        // The path-based check in validate_rename_operation handles the common cases
        // For edge cases, we rely on the direct inode check above
        Ok(false)
    }

    /// Validate rename operation parameters and permissions
    async fn validate_rename_operation(
        &self,
        old_path: &str,
        new_path: &str,
        old_parent_ino: i64,
        old_name: &str,
        _new_parent_ino: i64,
        new_name: &str,
    ) -> Result<(i64, FileAttr), VfsError> {
        // Validate source exists and get its attributes first
        let src_ino = self
            .meta_lookup_required(
                old_parent_ino,
                old_name,
                PathHint::some(format!("source '{}' not found", old_path)),
            )
            .await?;

        let src_attr = self
            .meta_stat_required(
                src_ino,
                PathHint::some(format!("source '{}' metadata not found", old_path)),
            )
            .await?;

        // Prevent renaming to the same location
        if old_path == new_path {
            // POSIX allows this as a no-op, so we return success
            // The caller should handle this gracefully
        }

        // Validate target name is not empty and doesn't contain invalid characters
        if new_name.is_empty() {
            return Err(VfsError::InvalidRenameTarget {
                path: PathHint::some("target name cannot be empty"),
            });
        }

        if new_name.contains('/') || new_name.contains('\0') {
            return Err(VfsError::InvalidRenameTarget {
                path: PathHint::some(format!(
                    "target name '{}' contains invalid characters",
                    new_name
                )),
            });
        }

        // Check for circular rename (directory into its own subdirectory)
        // Simple path-based check: if new_path starts with old_path/, it's circular
        if src_attr.kind == FileType::Dir {
            let old_path_with_slash = format!("{}/", old_path.trim_end_matches('/'));
            let new_path_normalized = new_path.trim_end_matches('/');

            if new_path_normalized.starts_with(&old_path_with_slash) {
                return Err(VfsError::CircularRename {
                    path: PathHint::some(format!(
                        "cannot move directory '{}' into its own subdirectory '{}'",
                        old_path, new_path
                    )),
                });
            }

            // Also check via inode if paths are different parents
            if _new_parent_ino != old_parent_ino
                && self
                    .is_circular_rename(src_ino, &src_attr, _new_parent_ino)
                    .await?
            {
                return Err(VfsError::CircularRename {
                    path: PathHint::some(format!(
                        "cannot move directory '{}' into its own subdirectory '{}'",
                        old_path, new_path
                    )),
                });
            }
        }

        // Check if source and destination are on the same filesystem
        // For now, we assume all operations are within the same filesystem
        // Future enhancement: check device IDs

        Ok((src_ino, src_attr))
    }

    /// Optimized rename within the same directory - avoids duplicate parent resolution
    async fn rename_same_directory(
        &self,
        dir: &str,
        old_name: &str,
        new_name: &str,
    ) -> Result<(), VfsError> {
        let parent_ino = self.resolve_parent_inode(dir).await?;
        let old_path = format!("{}{}{}", dir, if dir == "/" { "" } else { "/" }, old_name);
        let new_path = format!("{}{}{}", dir, if dir == "/" { "" } else { "/" }, new_name);

        // Validate the rename operation
        let (_src_ino, src_attr) = self
            .validate_rename_operation(
                &old_path, &new_path, parent_ino, old_name, parent_ino, new_name,
            )
            .await?;

        // Handle destination existence and replacement semantics
        let dst_path = format!("{}{}{}", dir, if dir == "/" { "" } else { "/" }, new_name);
        if let Some((dest_ino, dest_kind)) = self.meta_lookup_path(&dst_path).await? {
            // Handle replacement logic (same as in main rename function)
            match (src_attr.kind, dest_kind) {
                // Directory replacing directory
                (FileType::Dir, FileType::Dir) => {
                    let children = self.meta_readdir(dest_ino).await?;
                    if !children.is_empty() {
                        return Err(VfsError::DirectoryNotEmpty {
                            path: PathHint::some(format!(
                                "cannot replace non-empty directory '{}/{}'",
                                dir, new_name
                            )),
                        });
                    }
                    self.meta_rmdir(parent_ino, new_name).await?;
                }
                // Directory replacing file/symlink - not allowed
                (FileType::Dir, FileType::File) | (FileType::Dir, FileType::Symlink) => {
                    return Err(VfsError::IsADirectory {
                        path: PathHint::some(format!(
                            "cannot replace file '{}/{}' with directory",
                            dir, old_name
                        )),
                    });
                }
                // File/symlink replacing directory - not allowed
                (FileType::File, FileType::Dir) | (FileType::Symlink, FileType::Dir) => {
                    return Err(VfsError::IsADirectory {
                        path: PathHint::some(format!(
                            "cannot replace directory '{}/{}' with file",
                            dir, new_name
                        )),
                    });
                }
                // File/symlink replacing file/symlink - allowed
                _ => {
                    self.meta_unlink(parent_ino, new_name).await?;
                }
            }
        }

        // Perform the rename
        self.meta_rename(parent_ino, old_name, parent_ino, new_name.to_string())
            .await?;

        // Update cache
        self.state.modified.touch(parent_ino).await;

        Ok(())
    }

    /// Step 1: Resolve parent directory inode from path
    async fn resolve_parent_inode(&self, dir_path: &str) -> Result<i64, VfsError> {
        if dir_path == "/" {
            return Ok(self.core.root);
        }

        let (ino, kind) = self.meta_lookup_path_required(dir_path).await?;

        if kind != FileType::Dir {
            return Err(VfsError::NotADirectory {
                path: PathHint::some(dir_path),
            });
        }

        Ok(ino)
    }

    /// Step 2: Handle destination replacement according to POSIX semantics
    async fn handle_destination_replacement(
        &self,
        new_path: &str,
        old_path: &str,
        src_kind: FileType,
        new_parent_ino: i64,
        new_name: &str,
    ) -> Result<(), VfsError> {
        if let Some((dest_ino, dest_kind)) = self.meta_lookup_path(new_path).await? {
            match (src_kind, dest_kind) {
                // Directory → Directory: only if destination is empty
                (FileType::Dir, FileType::Dir) => {
                    let children = self.meta_readdir(dest_ino).await?;

                    if !children.is_empty() {
                        return Err(VfsError::DirectoryNotEmpty {
                            path: PathHint::some(format!(
                                "cannot replace non-empty directory '{}'",
                                new_path
                            )),
                        });
                    }

                    self.meta_rmdir(new_parent_ino, new_name).await?;
                }

                // Directory → File/Symlink: not allowed
                (FileType::Dir, FileType::File) | (FileType::Dir, FileType::Symlink) => {
                    return Err(VfsError::IsADirectory {
                        path: PathHint::some(format!(
                            "cannot replace file '{}' with directory '{}'",
                            new_path, old_path
                        )),
                    });
                }

                // File/Symlink → Directory: not allowed
                (FileType::File, FileType::Dir) | (FileType::Symlink, FileType::Dir) => {
                    return Err(VfsError::IsADirectory {
                        path: PathHint::some(format!(
                            "cannot replace directory '{}' with file '{}'",
                            new_path, old_path
                        )),
                    });
                }

                // File/Symlink → File/Symlink: allowed, remove destination
                (FileType::File, FileType::File)
                | (FileType::File, FileType::Symlink)
                | (FileType::Symlink, FileType::File)
                | (FileType::Symlink, FileType::Symlink) => {
                    self.meta_unlink(new_parent_ino, new_name).await?;
                }
            }
        }
        Ok(())
    }

    /// Step 3: Execute the rename and update metadata
    async fn execute_rename(
        &self,
        old_parent_ino: i64,
        old_name: &str,
        new_parent_ino: i64,
        new_name: String,
    ) -> Result<(), VfsError> {
        self.meta_rename(old_parent_ino, old_name, new_parent_ino, new_name)
            .await?;

        // Update modification tracking
        self.state.modified.touch(old_parent_ino).await;
        if old_parent_ino != new_parent_ino {
            self.state.modified.touch(new_parent_ino).await;
        }

        Ok(())
    }

    #[tracing::instrument(level = "trace", skip(self), fields(old, new))]
    pub async fn rename(&self, old: &str, new: &str) -> Result<(), VfsError> {
        // Step 1: Normalize and parse paths
        let old = Self::norm_path(old);
        let new = Self::norm_path(new);
        let (old_dir, old_name) = Self::split_dir_file(&old);
        let (new_dir, new_name) = Self::split_dir_file(&new);

        // Fast path: same directory rename
        if old_dir == new_dir {
            return self
                .rename_same_directory(&old_dir, &old_name, &new_name)
                .await;
        }

        // Step 2: Resolve parent directory inodes
        let old_parent_ino = self.resolve_parent_inode(&old_dir).await?;
        let new_parent_ino = self.resolve_parent_inode(&new_dir).await?;

        // Step 3: Validate the rename operation
        let (_src_ino, src_attr) = self
            .validate_rename_operation(
                &old,
                &new,
                old_parent_ino,
                &old_name,
                new_parent_ino,
                &new_name,
            )
            .await?;

        // Step 4: Handle destination replacement according to POSIX semantics
        self.handle_destination_replacement(&new, &old, src_attr.kind, new_parent_ino, &new_name)
            .await?;

        // Step 5: Ensure destination parent exists (create if needed)
        let new_dir_ino = self.mkdir_p(&new_dir).await?;

        // Step 6: Execute the rename operation
        self.execute_rename(old_parent_ino, &old_name, new_dir_ino, new_name)
            .await?;

        Ok(())
    }

    /// Rename files or directories with extended flags support.
    /// This is similar to Linux renameat2 syscall with additional flags.
    pub async fn rename_with_flags(
        &self,
        old: &str,
        new: &str,
        flags: RenameFlags,
    ) -> Result<(), VfsError> {
        if flags.exchange {
            return self.rename_exchange(old, new).await;
        }

        if flags.noreplace {
            return self.rename_noreplace(old, new).await;
        }

        // Default behavior - allow replacement
        self.rename(old, new).await
    }

    /// Rename without replacing the destination (RENAME_NOREPLACE).
    /// Returns an error if the destination already exists.
    pub async fn rename_noreplace(&self, old: &str, new: &str) -> Result<(), VfsError> {
        let old = Self::norm_path(old);
        let new = Self::norm_path(new);

        // Check if destination exists
        if self.meta_lookup_path(&new).await?.is_some() {
            return Err(VfsError::AlreadyExists {
                path: PathHint::some(format!("destination '{}' already exists", new)),
            });
        }

        // Use standard rename
        self.rename(&old, &new).await
    }

    /// Atomically exchange the source and destination (RENAME_EXCHANGE).
    /// Both source and destination must exist.
    pub async fn rename_exchange(&self, old: &str, new: &str) -> Result<(), VfsError> {
        let old = Self::norm_path(old);
        let new = Self::norm_path(new);

        // Both source and destination must exist
        let (old_dir, old_name) = Self::split_dir_file(&old);
        let (new_dir, new_name) = Self::split_dir_file(&new);

        // Resolve parents
        let old_parent_ino = self.resolve_parent_inode(&old_dir).await?;
        let new_parent_ino = self.resolve_parent_inode(&new_dir).await?;

        // Both entries must exist
        let _old_ino = self
            .meta_lookup_required(old_parent_ino, &old_name, PathHint::some(old.as_str()))
            .await?;

        let _new_ino = self
            .meta_lookup_required(new_parent_ino, &new_name, PathHint::some(new.as_str()))
            .await?;

        // Perform atomic exchange via store layer
        self.meta_rename_exchange(old_parent_ino, &old_name, new_parent_ino, &new_name)
            .await?;

        // Update cache
        self.state.modified.touch(old_parent_ino).await;
        if old_parent_ino != new_parent_ino {
            self.state.modified.touch(new_parent_ino).await;
        }

        Ok(())
    }

    /// Check if a rename operation would be allowed without actually performing it.
    pub async fn can_rename(&self, old: &str, new: &str) -> Result<(), VfsError> {
        let old = Self::norm_path(old);
        let new = Self::norm_path(new);
        let (old_dir, old_name) = Self::split_dir_file(&old);
        let (new_dir, new_name) = Self::split_dir_file(&new);

        // Validate basic parameters
        if old.is_empty() || new.is_empty() {
            return Err(VfsError::InvalidInput);
        }

        if new_name.is_empty() || new_name.contains('/') || new_name.contains('\0') {
            return Err(VfsError::InvalidFilename);
        }

        // Check source exists
        let old_parent_ino = self.resolve_parent_inode(&old_dir).await?;

        let src_ino = self
            .meta_lookup_required(old_parent_ino, &old_name, PathHint::some(old.as_str()))
            .await?;

        let src_attr = self
            .meta_stat_required(src_ino, PathHint::some(old.as_str()))
            .await?;

        // Check destination parent exists
        let _new_parent_ino = self.resolve_parent_inode(&new_dir).await?;

        // Check destination constraints
        if let Some((dest_ino, dest_kind)) = self.meta_lookup_path(&new).await? {
            let _dest_attr = self
                .meta_stat_required(dest_ino, PathHint::some(new.as_str()))
                .await?;

            match (src_attr.kind, dest_kind) {
                // Directory replacing directory
                (FileType::Dir, FileType::Dir) => {
                    let children = self.meta_readdir(dest_ino).await?;
                    if !children.is_empty() {
                        return Err(VfsError::DirectoryNotEmpty {
                            path: PathHint::some(new.as_str()),
                        });
                    }
                }
                // Directory replacing file/symlink
                (FileType::Dir, FileType::File) | (FileType::Dir, FileType::Symlink) => {
                    return Err(VfsError::IsADirectory {
                        path: PathHint::some(new.as_str()),
                    });
                }
                // File/symlink replacing directory
                (FileType::File, FileType::Dir) | (FileType::Symlink, FileType::Dir) => {
                    return Err(VfsError::IsADirectory {
                        path: PathHint::some(new.as_str()),
                    });
                }
                // File/symlink replacing file/symlink - allowed
                _ => {}
            }
        }

        Ok(())
    }

    /// Batch rename multiple files efficiently
    /// Returns a vector of results, one for each rename operation
    pub async fn rename_batch(
        &self,
        operations: Vec<(String, String)>,
    ) -> Vec<Result<(), VfsError>> {
        let mut results = Vec::with_capacity(operations.len());

        // Process operations sequentially for simplicity
        // In a more advanced implementation, we could parallelize non-conflicting operations
        for (old_path, new_path) in operations {
            let result = self.rename(&old_path, &new_path).await;
            results.push(result);
        }

        results
    }

    /// Truncate/extend file size (metadata only; holes are read as zeros).
    /// Shrinking does not eagerly reclaim block data.
    #[tracing::instrument(level = "trace", skip(self), fields(path, size))]
    pub async fn truncate(&self, path: &str, size: u64) -> Result<(), VfsError> {
        let path = Self::norm_path(path);

        let (ino, _) = self.meta_lookup_path_required(&path).await?;

        self.truncate_inode(ino, size).await
    }

    /// Truncate/extend file size by inode (metadata only; holes are read as zeros).
    /// Shrinking does not eagerly reclaim block data.
    pub async fn truncate_inode(&self, ino: i64, size: u64) -> Result<(), VfsError> {
        let handles = self.file_handles_for_inode(ino);
        let mut guards = Vec::with_capacity(handles.len());
        for handle in handles {
            guards.push(handle.lock_write().await);
        }

        self.state
            .writer
            .flush_required(ino as u64)
            .await
            .map_err(|_| VfsError::Other)?;

        self.meta_truncate(ino, size, self.core.layout.chunk_size)
            .await?;

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

    #[tracing::instrument(level = "trace", skip(self, req), fields(ino, flags = ?flags))]
    pub async fn set_attr(
        &self,
        ino: i64,
        req: &SetAttrRequest,
        flags: SetAttrFlags,
    ) -> Result<FileAttr, VfsError> {
        // Hold handle write guards across the ENTIRE truncate + meta_set_attr
        // sequence so that no concurrent write_ino / FUSE_WRITE_CACHE can modify
        // the inode between the truncate and the attribute read-back.  Dropping
        // the guards too early allowed a race where meta_set_attr could read back
        // a size extended by a concurrent commit, causing the FUSE setattr
        // response to carry a wrong file size and confusing the kernel page cache.
        let _guards = if let Some(size) = req.size {
            let handles = self.file_handles_for_inode(ino);
            let mut guards = Vec::with_capacity(handles.len());
            for handle in handles {
                guards.push(handle.lock_write().await);
            }

            self.state
                .writer
                .flush_required(ino as u64)
                .await
                .map_err(|_| VfsError::Other)?;
            self.meta_truncate(ino, size, self.core.layout.chunk_size)
                .await?;
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

            Some(guards)
        } else {
            None
        };

        let mut filtered = *req;
        filtered.size = None;

        let mut attr = self.meta_set_attr(ino, &filtered, flags).await?;

        // Ensure the returned attr carries exactly the requested truncation size.
        // The kernel trusts this value for truncate_pagecache decisions; a stale
        // or extended size here can cause it to keep or invalidate wrong pages.
        if let Some(size) = req.size {
            attr.size = size;
            if let Some(inode) = self.state.inodes.get(&ino) {
                inode.update_size(size);
            }
        }

        self.state.modified.touch(ino).await;
        self.state.handles.update_attr_for_inode(ino, &attr);

        // _guards dropped here — after meta_set_attr has read the correct state
        Ok(attr)
    }

    /// Change the permission bits of an inode (chmod).
    ///
    /// `new_mode` is masked to `0o777` — setuid, setgid, and sticky bits are
    /// stripped because SlayerFS does not implement those semantics.
    /// Returns `VfsError::NotFound` when the inode does not exist.
    #[tracing::instrument(level = "trace", skip(self), fields(ino, new_mode))]
    pub async fn chmod(&self, ino: i64, new_mode: u32) -> Result<FileAttr, VfsError> {
        let attr = self.meta_chmod(ino, new_mode).await?;

        self.state.modified.touch(ino).await;
        self.state.handles.update_attr_for_inode(ino, &attr);

        Ok(attr)
    }

    /// Change the owner and/or group of an inode (chown).
    ///
    /// Either `uid` or `gid` may be `None` to leave that field unchanged.
    /// Returns `VfsError::NotFound` when the inode does not exist.
    #[tracing::instrument(level = "trace", skip(self), fields(ino, ?uid, ?gid))]
    pub async fn chown(
        &self,
        ino: i64,
        uid: Option<u32>,
        gid: Option<u32>,
    ) -> Result<FileAttr, VfsError> {
        let attr = self.meta_chown(ino, uid, gid).await?;

        self.state.modified.touch(ino).await;
        self.state.handles.update_attr_for_inode(ino, &attr);

        Ok(attr)
    }

    /// Read data by file handle and offset.
    #[tracing::instrument(
        name = "VFS.read",
        level = "trace",
        skip(self),
        fields(fh, offset, len)
    )]
    pub async fn read(&self, fh: u64, offset: u64, len: usize) -> Result<Vec<u8>, VfsError> {
        if len == 0 {
            return Ok(Vec::new());
        }

        let handle = self.file_handle_required(fh)?;
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

        let handle = self.file_handle_required(fh)?;

        if !handle.flags.write {
            return Err(VfsError::PermissionDenied {
                path: PathHint::none(),
            });
        }

        tracing::trace!(fh, ino = handle.ino, offset, len = data.len(), "vfs.write");

        let written = handle.write(offset, data).await?;

        // Invalidate reader cache for the written range so subsequent reads
        // (including FUSE reads on kernel page-cache miss) see committed data
        // instead of a stale cached snapshot from before this write.
        let _ = self
            .state
            .reader
            .invalidate(handle.ino as u64, offset, data.len())
            .await;

        self.update_mtime_ctime(handle.ino).await?;
        self.state.modified.touch(handle.ino).await;

        tracing::trace!(fh, ino = handle.ino, written, "vfs.write_done");
        Ok(written)
    }

    /// Write data by inode directly (used by FUSE to avoid path resolution).
    pub async fn write_ino(&self, ino: i64, offset: u64, data: &[u8]) -> Result<usize, VfsError> {
        if data.is_empty() {
            return Ok(0);
        }

        let handles = self.file_handles_for_inode(ino);
        let mut guards = Vec::with_capacity(handles.len());
        for handle in handles {
            guards.push(handle.lock_write().await);
        }

        let attr = self.meta_stat_required(ino, PathHint::none()).await?;
        if attr.kind == FileType::Dir {
            return Err(VfsError::IsADirectory {
                path: PathHint::none(),
            });
        }
        if attr.kind != FileType::File {
            return Err(VfsError::InvalidInput);
        }

        let inode = self.ensure_inode_registered(ino).await?;
        let writer = self.state.writer.ensure_file(inode);
        let written = writer
            .write_at(offset, data)
            .await
            .map_err(VfsError::from)?;
        writer.flush().await.map_err(|_| VfsError::Other)?;

        // Invalidate reader cache for the written range so any subsequent
        // FUSE read (kernel page-cache miss) fetches the freshly committed
        // data instead of a stale cached zero-fill from a prior truncate.
        let _ = self
            .state
            .reader
            .invalidate(ino as u64, offset, data.len())
            .await;

        self.update_mtime_ctime(ino).await?;
        self.state.modified.touch(ino).await;
        drop(guards);
        Ok(written)
    }

    /// Copy a byte range between two opened file handles.
    ///
    /// This keeps the copy inside SlayerFS so we can serialize it with the
    /// inode write path instead of falling back to kernel/user-space emulation.
    pub async fn copy_file_range(
        &self,
        fh_in: u64,
        off_in: u64,
        fh_out: u64,
        off_out: u64,
        length: u64,
    ) -> Result<usize, VfsError> {
        if length == 0 {
            return Ok(0);
        }

        let len = usize::try_from(length).map_err(|_| VfsError::InvalidInput)?;
        let src = self.file_handle_required(fh_in)?;
        let dst = self.file_handle_required(fh_out)?;

        if !src.flags.read {
            return Err(VfsError::PermissionDenied {
                path: PathHint::none(),
            });
        }
        if !dst.flags.write {
            return Err(VfsError::PermissionDenied {
                path: PathHint::none(),
            });
        }

        let mut locked = Vec::new();
        let mut unique = BTreeMap::new();
        for handle in self.file_handles_for_inode(src.ino) {
            unique.insert(handle.fh, handle);
        }
        for handle in self.file_handles_for_inode(dst.ino) {
            unique.insert(handle.fh, handle);
        }
        for handle in unique.into_values() {
            locked.push(handle.lock_write().await);
        }

        self.state.writer.flush_if_exists(src.ino as u64).await;
        if dst.ino != src.ino {
            self.state.writer.flush_if_exists(dst.ino as u64).await;
        }

        let src_attr = self.meta_stat_required(src.ino, PathHint::none()).await?;
        let dst_attr = self.meta_stat_required(dst.ino, PathHint::none()).await?;
        let src_guard = self.open_guard(src.ino, src_attr, true, false).await?;
        let dst_guard = self.open_guard(dst.ino, dst_attr, false, true).await?;

        // Read the full source snapshot before writing so same-file overlap keeps
        // copy_file_range semantics close to a memmove-style copy.
        let data = src_guard.read(off_in, len).await?;
        let written = dst_guard.write(off_out, &data).await?;

        drop(dst_guard);
        drop(src_guard);
        drop(locked);

        Ok(written)
    }

    /// Allocate a per-file handle, returning the opaque fh id.
    #[tracing::instrument(level = "trace", skip(self), fields(ino, read, write))]
    pub async fn open(
        &self,
        ino: i64,
        attr: FileAttr,
        read: bool,
        write: bool,
    ) -> Result<u64, VfsError> {
        let mut latest_attr = attr;

        // Retrieve the latest attr for close-to-open semantics.
        match self.meta_stat_fresh(ino).await {
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

    /// Allocate a file handle and return a guard that auto-closes on drop.
    pub async fn open_guard(
        &self,
        ino: i64,
        attr: FileAttr,
        read: bool,
        write: bool,
    ) -> Result<FileGuard<S, M>, VfsError> {
        let fh = self.open(ino, attr, read, write).await?;
        Ok(FileGuard::new(self.clone(), fh))
    }

    /// Release a previously allocated file handle.
    pub async fn close(&self, fh: u64) -> Result<(), VfsError> {
        // Note that we cannot hold the lock during the entire function, because `handle.flush()` is a I/O operation.
        let handle = self.file_handle_required(fh)?;

        tracing::trace!(
            fh,
            ino = handle.ino,
            write = handle.flags.write,
            "vfs.close"
        );
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

        tracing::trace!(fh, ino = handle.ino, "vfs.close_done");
        Ok(())
    }

    /// Shared implementation for flush and fsync: conditionally flushes pending writes
    /// and updates timestamps. Returns the inode number for logging by the caller.
    async fn flush_and_sync_handle(&self, fh: u64) -> Result<i64, VfsError> {
        let handle = self.file_handle_required(fh)?;

        if handle.flags.write {
            handle.flush().await.map_err(|_| VfsError::Other)?;
        }

        self.update_timestamps_on_flush(handle.ino).await?;
        Ok(handle.ino)
    }

    /// Flush pending writes for a file handle.
    pub async fn flush(&self, fh: u64) -> Result<(), VfsError> {
        let handle = self.file_handle_required(fh)?;

        tracing::trace!(
            fh,
            ino = handle.ino,
            write = handle.flags.write,
            "vfs.flush"
        );
        let ino = self.flush_and_sync_handle(fh).await?;
        tracing::trace!(fh, ino, "vfs.flush_done");
        Ok(())
    }

    /// Sync file content (fsync): flush pending writes.
    pub async fn fsync(&self, fh: u64, _datasync: bool) -> Result<(), VfsError> {
        let handle = self.file_handle_required(fh)?;

        tracing::trace!(
            fh,
            ino = handle.ino,
            write = handle.flags.write,
            "vfs.fsync"
        );

        let ino = self.flush_and_sync_handle(fh).await?;
        tracing::trace!(fh, ino, "vfs.fsync_done");
        Ok(())
    }

    /// Open a directory handle for reading. Returns the file handle ID.
    /// This pre-loads all directory entries and starts background batch prefetch for attributes.
    #[tracing::instrument(level = "trace", skip(self), fields(ino))]
    pub async fn opendir(&self, ino: i64) -> Result<u64, VfsError> {
        let handle = self.meta_opendir(ino).await?;
        let fh = self.state.handles.allocate_dir(handle);

        Ok(fh)
    }

    /// Close a directory handle
    pub fn closedir(&self, fh: u64) -> Result<(), VfsError> {
        let handle = self.release_dir_handle_required(fh)?;

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
        let handle = self.dir_handle(fh)?;

        Some(handle.get_entries(offset))
    }

    /// Update cached information about a handle (e.g. last observed offset).
    pub(crate) fn touch_handle_offset(&self, fh: u64, offset: u64) -> Result<(), VfsError> {
        let handle = self.file_handle_required(fh)?;
        handle.update_offset(offset);

        Ok(())
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
        self.meta_get_plock(inode, query).await
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
        self.meta_set_plock(inode, owner, block, lock_type, range, pid)
            .await
    }

    /// Set xattr for a given inode.
    pub async fn set_xattr_ino(
        &self,
        inode: i64,
        name: &str,
        value: &[u8],
        flags: u32,
    ) -> Result<(), VfsError> {
        self.meta_set_xattr(inode, name, value, flags).await
    }

    /// Get xattr for a given inode.
    pub async fn get_xattr_ino(&self, inode: i64, name: &str) -> Result<Option<Vec<u8>>, VfsError> {
        self.meta_get_xattr(inode, name).await
    }

    /// List xattr names for a given inode.
    pub async fn list_xattr_ino(&self, inode: i64) -> Result<Vec<String>, VfsError> {
        self.meta_list_xattr(inode).await
    }

    /// Remove xattr for a given inode.
    pub async fn remove_xattr_ino(&self, inode: i64, name: &str) -> Result<(), VfsError> {
        self.meta_remove_xattr(inode, name).await
    }

    /// Set ACL rule for a given inode.
    pub async fn set_acl_ino(&self, inode: i64, rule: AclRule) -> Result<(), VfsError> {
        self.meta_set_acl(inode, rule).await
    }

    /// Get ACL rule for a given inode.
    pub async fn get_acl_ino(
        &self,
        inode: i64,
        acl_type: u8,
        acl_id: u32,
    ) -> Result<Option<AclRule>, VfsError> {
        self.meta_get_acl(inode, acl_type, acl_id).await
    }

    /// Resolves a normalized path to its inode number, returning NotFound if absent.
    async fn lookup_path_to_ino(&self, path: &str) -> Result<i64, VfsError> {
        let (inode, _) = self.meta_lookup_path_required(path).await?;
        Ok(inode)
    }

    /// Get file lock information by path.
    pub async fn get_plock(
        &self,
        path: &str,
        query: &FileLockQuery,
    ) -> Result<FileLockInfo, VfsError> {
        let path = Self::norm_path(path);
        let inode = self.lookup_path_to_ino(&path).await?;
        self.meta_get_plock(inode, query).await
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
        let inode = self.lookup_path_to_ino(&path).await?;
        self.meta_set_plock(inode, owner, block, lock_type, range, pid)
            .await
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

    /// Get file system statistics (total/available space and inodes).
    pub async fn stat_fs(&self) -> Result<StatFsSnapshot, VfsError> {
        self.meta_stat_fs().await
    }

    async fn ensure_inode_registered(&self, ino: i64) -> Result<Arc<Inode>, VfsError> {
        // Fast path to check whether there is an existing inode.
        if let Some(inode) = self.state.inodes.get(&ino) {
            return Ok(Arc::clone(inode.value()));
        }

        match self.lock_inode(ino) {
            Entry::Occupied(entry) => Ok(Arc::clone(entry.get())),
            Entry::Vacant(entry) => {
                let attr = self.meta_stat_required(ino, PathHint::none()).await?;
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

/// RAII guard for file handles that ensures close on drop.
pub struct FileGuard<S, M>
where
    S: BlockStore + Send + Sync + 'static,
    M: MetaLayer + Send + Sync + 'static,
{
    vfs: VFS<S, M>,
    fh: u64,
    closed: bool,
}

impl<S, M> FileGuard<S, M>
where
    S: BlockStore + Send + Sync + 'static,
    M: MetaLayer + Send + Sync + 'static,
{
    fn new(vfs: VFS<S, M>, fh: u64) -> Self {
        Self {
            vfs,
            fh,
            closed: false,
        }
    }

    pub fn fh(&self) -> u64 {
        self.fh
    }

    pub async fn read(&self, offset: u64, len: usize) -> Result<Vec<u8>, VfsError> {
        self.vfs.read(self.fh, offset, len).await
    }

    pub async fn write(&self, offset: u64, data: &[u8]) -> Result<usize, VfsError> {
        self.vfs.write(self.fh, offset, data).await
    }

    pub async fn close(mut self) -> Result<(), VfsError> {
        self.closed = true;
        self.vfs.close(self.fh).await
    }
}

impl<S, M> Drop for FileGuard<S, M>
where
    S: BlockStore + Send + Sync + 'static,
    M: MetaLayer + Send + Sync + 'static,
{
    fn drop(&mut self) {
        if self.closed {
            return;
        }
        close_handle_best_effort(self.vfs.clone(), self.fh);
    }
}

fn close_handle_best_effort<S, M>(vfs: VFS<S, M>, fh: u64)
where
    S: BlockStore + Send + Sync + 'static,
    M: MetaLayer + Send + Sync + 'static,
{
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        handle.spawn(async move {
            let _ = vfs.close(fh).await;
        });
        return;
    }

    let _ = std::thread::Builder::new()
        .name("slayerfs-vfs-close".to_string())
        .spawn(move || {
            if let Ok(rt) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                let _ = rt.block_on(vfs.close(fh));
            }
        });
}

#[cfg(test)]
mod tests;
