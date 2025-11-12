//! FUSE/SDK-friendly VFS with path-based create/mkdir/read/write/readdir/stat support.

use crate::chuck::chunk::ChunkLayout;
use crate::chuck::reader::ChunkReader;
use crate::chuck::store::BlockStore;
use crate::chuck::writer::ChunkWriter;
use crate::meta::MetaStore;
use dashmap::{DashMap, Entry};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

// Re-export types from meta::store for convenience
pub use crate::meta::store::{DirEntry, FileAttr, FileType};
use crate::vfs::handles::{FileHandle, HandleFlags};
use crate::vfs::inode::Inode;
use crate::vfs::io::FileRegistry;

struct HandleRegistry {
    handles: DashMap<i64, Vec<FileHandle>>,
    handle_ino: DashMap<u64, i64>,
    next_fh: AtomicU64,
}

impl HandleRegistry {
    fn new() -> Self {
        Self {
            handles: DashMap::new(),
            handle_ino: DashMap::new(),
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
    M: MetaStore,
{
    handles: HandleRegistry,
    files: FileRegistry<S, M>,
    modified: ModifiedTracker,
}

impl<S, M> VfsState<S, M>
where
    S: BlockStore,
    M: MetaStore,
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
    M: MetaStore,
{
    layout: ChunkLayout,
    meta: Arc<M>,
    chunk_io: Arc<ChunkIoFactory<S, M>>,
    root: i64,
}

impl<S, M> VfsCore<S, M>
where
    S: BlockStore,
    M: MetaStore,
{
    fn new(layout: ChunkLayout, store: Arc<S>, meta: Arc<M>, root: i64) -> Self {
        let chunk_io = Arc::new(ChunkIoFactory::new(
            layout,
            Arc::clone(&store),
            Arc::clone(&meta),
        ));

        Self {
            layout,
            meta,
            chunk_io,
            root,
        }
    }
}

#[allow(unused)]
#[allow(clippy::upper_case_acronyms)]
#[derive(Clone)]
pub struct VFS<S, M>
where
    S: BlockStore,
    M: MetaStore,
{
    core: Arc<VfsCore<S, M>>,
    state: Arc<VfsState<S, M>>,
}

#[allow(dead_code)]
impl<S, M> VFS<S, M>
where
    S: BlockStore,
    M: MetaStore,
{
    pub async fn new(layout: ChunkLayout, store: S, meta: M) -> Result<Self, String> {
        let store = Arc::new(store);
        let meta = Arc::new(meta);

        meta.initialize().await.map_err(|e| e.to_string())?;

        let root_ino = meta.root_ino();

        let core = Arc::new(VfsCore::new(
            layout,
            Arc::clone(&store),
            Arc::clone(&meta),
            root_ino,
        ));
        let state = Arc::new(VfsState::new());

        Ok(Self { core, state })
    }

    pub fn root_ino(&self) -> i64 {
        self.core.root
    }

    /// get the node's parent inode.
    pub async fn parent_of(&self, ino: i64) -> Option<i64> {
        self.core.meta.get_parent(ino).await.ok().flatten()
    }

    /// get the node's fullpath.
    pub async fn path_of(&self, ino: i64) -> Option<String> {
        self.core.meta.get_path(ino).await.ok().flatten()
    }

    /// get the node's child inode by name.
    pub async fn child_of(&self, parent: i64, name: &str) -> Option<i64> {
        self.core.meta.lookup(parent, name).await.ok().flatten()
    }

    pub async fn stat_ino(&self, ino: i64) -> Option<FileAttr> {
        self.core.meta.stat(ino).await.ok().flatten()
    }

    /// List directory entries by inode
    pub async fn readdir_ino(&self, ino: i64) -> Option<Vec<DirEntry>> {
        let meta_entries = self.core.meta.readdir(ino).await.ok()?;

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
    pub async fn mkdir_p(&self, path: &str) -> Result<i64, String> {
        let path = Self::norm_path(path);
        if &path == "/" {
            return Ok(self.core.root);
        }
        if let Ok(Some((ino, _attr))) = self.core.meta.lookup_path(&path).await {
            return Ok(ino);
        }
        let mut cur_ino = self.core.root;
        for part in path.trim_start_matches('/').split('/') {
            if part.is_empty() {
                continue;
            }
            match self.core.meta.lookup(cur_ino, part).await {
                Ok(Some(ino)) => {
                    if let Ok(Some(attr)) = self.core.meta.stat(ino).await
                        && attr.kind != FileType::Dir
                    {
                        return Err("not a directory".into());
                    }
                    cur_ino = ino;
                }
                _ => {
                    let ino = self
                        .core
                        .meta
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
        if let Ok(Some(ino)) = self.core.meta.lookup(dir_ino, &name).await
            && let Ok(Some(attr)) = self.core.meta.stat(ino).await
        {
            return if attr.kind == FileType::Dir {
                Err("is a directory".into())
            } else {
                Ok(ino)
            };
        }

        let ino = self
            .core
            .meta
            .create_file(dir_ino, name.clone())
            .await
            .map_err(|e| e.to_string())?;
        self.state.modified.touch(dir_ino).await;
        self.state.modified.touch(ino).await;
        Ok(ino)
    }

    /// Fetch a file's attributes (kind/size come from the MetaStore); returns None when missing.
    pub async fn stat(&self, path: &str) -> Option<FileAttr> {
        let path = Self::norm_path(path);
        let (ino, _) = self.core.meta.lookup_path(&path).await.ok()??;
        let meta_attr = self.core.meta.stat(ino).await.ok().flatten()?;
        Some(meta_attr)
    }

    /// List directory entries; returns None if the path is missing or not a directory.
    /// `.` and `..` are not included.
    pub async fn readdir(&self, path: &str) -> Option<Vec<DirEntry>> {
        let path = Self::norm_path(path);
        let (ino, _) = self.core.meta.lookup_path(&path).await.ok()??;

        let meta_entries = self.core.meta.readdir(ino).await.ok()?;

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
        matches!(self.core.meta.lookup_path(&path).await, Ok(Some(_)))
    }

    /// Remove a regular file (directories are not supported here).
    pub async fn unlink(&self, path: &str) -> Result<(), String> {
        let path = Self::norm_path(path);
        let (dir, name) = Self::split_dir_file(&path);

        let parent_ino = if &dir == "/" {
            self.core.root
        } else {
            self.core
                .meta
                .lookup_path(&dir)
                .await
                .map_err(|e| e.to_string())?
                .ok_or_else(|| "parent not found".to_string())?
                .0
        };

        let ino = self
            .core
            .meta
            .lookup(parent_ino, &name)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "not found".to_string())?;

        let attr = self
            .core
            .meta
            .stat(ino)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "not found".to_string())?;

        if attr.kind != FileType::File {
            return Err("is a directory".into());
        }

        self.core
            .meta
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
                .meta
                .lookup_path(&dir)
                .await
                .map_err(|e| e.to_string())?
                .ok_or_else(|| "parent not found".to_string())?
                .0
        };

        let ino = self
            .core
            .meta
            .lookup(parent_ino, &name)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "not found".to_string())?;

        let attr = self
            .core
            .meta
            .stat(ino)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "not found".to_string())?;

        if attr.kind != FileType::Dir {
            return Err("not a directory".into());
        }

        let children = self
            .core
            .meta
            .readdir(ino)
            .await
            .map_err(|e| e.to_string())?;
        if !children.is_empty() {
            return Err("directory not empty".into());
        }

        self.core
            .meta
            .rmdir(parent_ino, &name)
            .await
            .map_err(|e| e.to_string())?;
        self.state.modified.touch(parent_ino).await;
        self.state.modified.touch(ino).await;

        Ok(())
    }

    /// Rename files or directories. The destination must not exist; parent directories are created as needed.
    pub async fn rename(&self, old: &str, new: &str) -> Result<(), String> {
        let old = Self::norm_path(old);
        let new = Self::norm_path(new);
        let (old_dir, old_name) = Self::split_dir_file(&old);
        let (new_dir, new_name) = Self::split_dir_file(&new);

        if self
            .core
            .meta
            .lookup_path(&new)
            .await
            .ok()
            .flatten()
            .is_some()
        {
            return Err("target exists".into());
        }

        let old_parent_ino = if &old_dir == "/" {
            self.core.root
        } else {
            self.core
                .meta
                .lookup_path(&old_dir)
                .await
                .map_err(|e| e.to_string())?
                .ok_or_else(|| "parent not found".to_string())?
                .0
        };

        let new_dir_ino = self.mkdir_p(&new_dir).await?;

        self.core
            .meta
            .rename(old_parent_ino, &old_name, new_dir_ino, new_name)
            .await
            .map_err(|e| e.to_string())?;

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
            .meta
            .lookup_path(&path)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "not found".to_string())?;
        if let Some(inode) = self.state.files.inode(ino) {
            inode.update_size(size);
        }
        self.core
            .meta
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
            .meta
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

        let guard = writer.lock().await;
        let written = guard.write(offset, data).await.map_err(|e| e.to_string())?;

        let target_size = offset + data.len() as u64;
        if target_size > inode.file_size() {
            inode.update_size(target_size);
            self.core
                .meta
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
            .meta
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
                    .meta
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
    use crate::meta::create_meta_store_from_url;
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

        let meta = create_meta_store_from_url("sqlite::memory:").await.unwrap();
        let fs = VFS::new(layout, store, meta).await.unwrap();

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

        let meta = create_meta_store_from_url("sqlite::memory:").await.unwrap();
        let fs = VFS::new(layout, store, meta).await.unwrap();

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
        let meta = create_meta_store_from_url("sqlite::memory:").await.unwrap();
        let fs = VFS::new(layout, store, meta).await.unwrap();

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
        let meta = create_meta_store_from_url("sqlite::memory:").await.unwrap();
        let fs = VFS::new(layout, store, meta).await.unwrap();
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
