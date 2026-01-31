//! SDK interface: simplified filesystem APIs for applications/SDKs (JuiceFS-style).
//!
//! Goals:
//! - Path-level APIs: mkdir_p/create/read/write/readdir/stat
//! - Pluggable backend: reuse Fs-level BlockStore and MetaStore
//! - Provide a convenient LocalFs constructor
//!
//! All methods return `io::Result<T>` for consistent error handling.

use crate::chuck::chunk::ChunkLayout;
use crate::chuck::store::BlockStore;
use crate::fs::{FileSystem, FileSystemConfig, OpenFlags};
use crate::meta::MetaStore;
use crate::meta::factory::create_meta_store_from_url;
use crate::meta::file_lock::{FileLockInfo, FileLockQuery, FileLockRange, FileLockType};
use crate::meta::store::{
    DirEntry, FileAttr, FileType, SetAttrFlags, SetAttrRequest, StatFsSnapshot,
};
use crate::meta::stores::DatabaseMetaStore;
use std::future::Future;
use std::io;
use std::path::Path;
use std::time::Duration;
use tokio::time::sleep;

const DEADLOCK_RETRY_MAX: usize = 5;
const DEADLOCK_RETRY_BASE_MS: u64 = 20;

fn is_deadlock_error(err: &io::Error) -> bool {
    if err.kind() == io::ErrorKind::Deadlock {
        return true;
    }
    err.to_string().to_lowercase().contains("deadlock")
}

fn deadlock_backoff(attempt: usize) -> Duration {
    let shift = attempt.min(6) as u32;
    Duration::from_millis(DEADLOCK_RETRY_BASE_MS.saturating_mul(1u64 << shift))
}

/// SDK client parametrized by its backend.
pub struct VfsClient<S: BlockStore + Send + Sync + 'static, M: MetaStore + 'static> {
    fs: FileSystem<S, M>,
}

#[allow(unused)]
impl<S: BlockStore + Send + Sync + 'static, M: MetaStore + 'static> VfsClient<S, M> {
    pub async fn new(layout: ChunkLayout, store: S, meta: M) -> io::Result<Self> {
        let fs = FileSystem::new(layout, store, meta).await?;
        Ok(Self { fs })
    }
}

#[allow(unused)]
impl<S: BlockStore + Send + Sync + 'static, M: MetaStore + 'static> VfsClient<S, M> {
    pub fn from_filesystem(fs: FileSystem<S, M>) -> Self {
        Self { fs }
    }

    /// Create directories recursively (like `mkdir -p`).
    pub async fn mkdir_p(&self, path: &str) -> io::Result<()> {
        self.fs.mkdir_all(path).await
    }

    /// Create a single directory (non-recursive).
    pub async fn mkdir(&self, path: &str) -> io::Result<()> {
        self.fs.mkdir(path).await
    }

    /// Create a file. If `create_new` is true, fails if file already exists.
    pub async fn create_file(&self, path: &str, create_new: bool) -> io::Result<()> {
        let flags = if create_new {
            OpenFlags::create_new()
        } else {
            OpenFlags::create_write()
        };
        let _ = self.fs.open(path, flags).await?;
        Ok(())
    }

    /// Write data at the specified offset.
    pub async fn write_at(&self, path: &str, offset: u64, data: &[u8]) -> io::Result<usize> {
        self.fs.write_at(path, offset, data).await
    }

    /// Read data at the specified offset.
    pub async fn read_at(&self, path: &str, offset: u64, len: usize) -> io::Result<Vec<u8>> {
        self.fs.read_at(path, offset, len).await
    }

    /// Read directory entries.
    pub async fn readdir(&self, path: &str) -> io::Result<Vec<DirEntry>> {
        self.fs.readdir(path).await
    }

    /// Get file/directory attributes.
    pub async fn stat(&self, path: &str) -> io::Result<FileAttr> {
        self.fs.stat(path).await.map(|fi| fi.attr().clone())
    }

    /// Remove a file.
    pub async fn unlink(&self, path: &str) -> io::Result<()> {
        self.retry_on_deadlock(|| self.fs.unlink(path)).await
    }

    /// Remove an empty directory.
    pub async fn rmdir(&self, path: &str) -> io::Result<()> {
        self.retry_on_deadlock(|| self.fs.rmdir(path)).await
    }

    /// Rename a file or directory.
    pub async fn rename(&self, old: &str, new: &str) -> io::Result<()> {
        self.retry_on_deadlock(|| self.fs.rename(old, new)).await
    }

    /// Truncate a file to the specified size.
    pub async fn truncate(&self, path: &str, size: u64) -> io::Result<()> {
        self.retry_on_deadlock(|| self.fs.truncate(path, size))
            .await
    }

    /// Check whether a path exists.
    pub async fn exists(&self, path: &str) -> bool {
        self.fs.exists(path).await
    }

    /// Set file/directory attributes (chmod, chown, utime).
    pub async fn set_attr(
        &self,
        path: &str,
        req: &SetAttrRequest,
        flags: SetAttrFlags,
    ) -> io::Result<FileAttr> {
        self.fs.set_attr(path, req, flags).await
    }

    /// Get file attributes without following symlinks.
    pub async fn lstat(&self, path: &str) -> io::Result<FileAttr> {
        self.fs.lstat(path).await.map(|fi| fi.attr().clone())
    }

    /// Recursively remove a directory and all its contents.
    pub async fn remove_dir_all(&self, path: &str) -> io::Result<()> {
        if path.trim_matches('/').is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "cannot remove filesystem root",
            ));
        }
        self.remove_dir_all_recursive(path).await
    }

    async fn remove_dir_all_recursive(&self, path: &str) -> io::Result<()> {
        let entries = self.fs.readdir(path).await?;
        for entry in entries {
            let child_path = if path == "/" {
                format!("/{}", entry.name)
            } else {
                format!("{}/{}", path, entry.name)
            };
            match entry.kind {
                FileType::Dir => {
                    Box::pin(self.remove_dir_all_recursive(&child_path)).await?;
                }
                _ => {
                    self.unlink(&child_path).await?;
                }
            }
        }
        self.rmdir(path).await
    }

    async fn retry_on_deadlock<T, Fut>(&self, mut op: impl FnMut() -> Fut) -> io::Result<T>
    where
        Fut: Future<Output = io::Result<T>>,
    {
        for attempt in 0..DEADLOCK_RETRY_MAX {
            match op().await {
                Ok(value) => return Ok(value),
                Err(err) if is_deadlock_error(&err) && attempt + 1 < DEADLOCK_RETRY_MAX => {
                    sleep(deadlock_backoff(attempt)).await;
                }
                Err(err) => return Err(err),
            }
        }
        unreachable!("retry_on_deadlock should return or error before exhausting attempts")
    }

    /// Get file system statistics (total/available space and inodes).
    pub async fn stat_fs(&self) -> io::Result<StatFsSnapshot> {
        let snapshot = self.fs.stat_fs().await?;
        Ok(StatFsSnapshot {
            total_space: snapshot.total_space,
            available_space: snapshot.avail_space,
            used_inodes: snapshot.used_inodes,
            available_inodes: snapshot.avail_inodes,
        })
    }

    /// Create a hard link.
    pub async fn link(&self, existing: &str, link_path: &str) -> io::Result<FileAttr> {
        self.fs.link(existing, link_path).await?;
        self.fs.stat(link_path).await.map(|fi| fi.attr().clone())
    }

    /// Create a symbolic link.
    pub async fn symlink(&self, link_path: &str, target: &str) -> io::Result<FileAttr> {
        self.fs.symlink(link_path, target).await?;
        self.fs.lstat(link_path).await.map(|fi| fi.attr().clone())
    }

    /// Read the target of a symbolic link.
    pub async fn readlink(&self, path: &str) -> io::Result<String> {
        self.fs.readlink(path).await
    }

    /// Get file lock information for a given path and query.
    pub async fn get_plock(&self, path: &str, query: &FileLockQuery) -> io::Result<FileLockInfo> {
        self.fs.get_plock(path, query).await
    }

    /// Set file lock for a given path.
    pub async fn set_plock(
        &self,
        path: &str,
        owner: i64,
        block: bool,
        lock_type: FileLockType,
        range: FileLockRange,
        pid: u32,
    ) -> io::Result<()> {
        self.fs
            .set_plock(path, owner, block, lock_type, range, pid)
            .await
    }
}

// ============== Convenience builder (LocalFs backend) ==============

use crate::cadapter::client::ObjectClient;
use crate::cadapter::localfs::LocalFsBackend;
use crate::chuck::store::ObjectBlockStore;
use std::sync::Arc;

#[allow(dead_code)]
pub type LocalClient = VfsClient<ObjectBlockStore<LocalFsBackend>, DatabaseMetaStore>;

#[allow(dead_code)]
impl LocalClient {
    #[allow(dead_code)]
    pub async fn new_local<P: AsRef<Path>>(root: P, layout: ChunkLayout) -> io::Result<Self> {
        let client = ObjectClient::new(LocalFsBackend::new(root));
        let meta_handle = create_meta_store_from_url("sqlite::memory:")
            .await
            .map_err(io::Error::other)?;
        let meta = meta_handle.store();
        let meta_layer = meta_handle.layer();
        let store = Arc::new(ObjectBlockStore::new(client));
        let fs = FileSystem::from_components(
            layout,
            Arc::clone(&store),
            meta,
            meta_layer,
            FileSystemConfig::default(),
        )?;
        Ok(VfsClient { fs })
    }

    #[allow(dead_code)]
    pub async fn new_local_with_config<P: AsRef<Path>>(
        root: P,
        layout: ChunkLayout,
        config: FileSystemConfig,
    ) -> io::Result<Self> {
        let client = ObjectClient::new(LocalFsBackend::new(root));
        let meta_handle = create_meta_store_from_url("sqlite::memory:")
            .await
            .map_err(io::Error::other)?;
        let meta = meta_handle.store();
        let meta_layer = meta_handle.layer();
        let store = Arc::new(ObjectBlockStore::new(client));
        let fs = FileSystem::from_components(layout, Arc::clone(&store), meta, meta_layer, config)?;
        Ok(VfsClient { fs })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::CallerIdentity;
    use crate::vfs::fs::FileType;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_sdk_local_basic() {
        let layout = ChunkLayout::default();
        let tmp = tempdir().unwrap();
        let config = FileSystemConfig::default().with_caller(CallerIdentity::root());
        let cli = LocalClient::new_local_with_config(tmp.path(), layout, config)
            .await
            .expect("init LocalClient");

        cli.mkdir_p("/a/b").await.unwrap();
        cli.create_file("/a/b/hello.txt", false).await.unwrap();

        let half = (layout.block_size / 2) as usize;
        let len = layout.block_size as usize + half;
        let mut data = vec![0u8; len];
        for (i, b) in data.iter_mut().enumerate().take(len) {
            *b = (i % 251) as u8;
        }
        cli.write_at("/a/b/hello.txt", half as u64, &data)
            .await
            .unwrap();

        let out = cli
            .read_at("/a/b/hello.txt", half as u64, len)
            .await
            .unwrap();
        assert_eq!(out, data);

        let ent = cli.readdir("/a/b").await.unwrap();
        assert!(ent.iter().any(|e| e.name == "hello.txt"));

        let st = cli.stat("/a/b/hello.txt").await.unwrap();
        assert!(st.size >= len as u64);
    }

    #[tokio::test]
    async fn test_sdk_local_ops_extras() {
        let layout = ChunkLayout::default();
        let tmp = tempdir().unwrap();
        let config = FileSystemConfig::default().with_caller(CallerIdentity::root());
        let cli = LocalClient::new_local_with_config(tmp.path(), layout, config)
            .await
            .expect("init LocalClient");

        cli.mkdir_p("/x/y").await.unwrap();
        cli.create_file("/x/y/a.txt", false).await.unwrap();
        cli.rename("/x/y/a.txt", "/x/y/b.txt").await.unwrap();
        cli.truncate("/x/y/b.txt", (layout.block_size * 2) as u64)
            .await
            .unwrap();
        let st = cli.stat("/x/y/b.txt").await.unwrap();
        assert!(st.size >= (layout.block_size * 2) as u64);
        cli.unlink("/x/y/b.txt").await.unwrap();
        // Directory is empty, so removal is allowed
        cli.rmdir("/x/y").await.unwrap();
    }

    #[tokio::test]
    async fn test_sdk_local_links() {
        let layout = ChunkLayout::default();
        let tmp = tempdir().unwrap();
        let config = FileSystemConfig::default().with_caller(CallerIdentity::root());
        let cli = LocalClient::new_local_with_config(tmp.path(), layout, config)
            .await
            .expect("init LocalClient");

        cli.mkdir_p("/links").await.unwrap();
        cli.create_file("/links/original.txt", false).await.unwrap();
        cli.write_at("/links/original.txt", 0, b"payload")
            .await
            .unwrap();

        let orig_attr = cli.stat("/links/original.txt").await.unwrap();
        let link_res = cli.link("/links/original.txt", "/links/hard.txt").await;
        let mut hard_created = false;
        let mut hard_path = "/links/hard.txt".to_string();
        if let Ok(hard_attr) = &link_res {
            assert_eq!(hard_attr.ino, orig_attr.ino);
            assert!(hard_attr.nlink >= 2);
            hard_created = true;

            cli.mkdir_p("/links/sub").await.unwrap();
            cli.rename("/links/hard.txt", "/links/sub/hard-renamed.txt")
                .await
                .unwrap();
            hard_path = "/links/sub/hard-renamed.txt".to_string();

            let renamed_attr = cli.stat(&hard_path).await.unwrap();
            assert_eq!(renamed_attr.ino, orig_attr.ino);
            assert!(renamed_attr.nlink >= 2);

            let legacy = cli.stat("/links/hard.txt").await;
            assert!(legacy.is_err());
        } else if let Err(err) = &link_res {
            // io::Error doesn't have Unsupported/Other variants, check error kind
            assert!(
                matches!(
                    err.kind(),
                    io::ErrorKind::Unsupported | io::ErrorKind::Other
                ),
                "unexpected hard-link error: {err:?}"
            );
        }

        let sym_attr = cli
            .symlink("/links/original.symlink", "/links/original.txt")
            .await
            .unwrap();
        assert_eq!(sym_attr.kind, FileType::Symlink);
        let target = cli.readlink("/links/original.symlink").await.unwrap();
        assert_eq!(target, "/links/original.txt");

        cli.unlink("/links/original.symlink").await.unwrap();
        if hard_created {
            cli.unlink("/links/original.txt").await.unwrap();
            let remaining_attr = cli.stat(&hard_path).await.unwrap();
            assert_eq!(remaining_attr.ino, orig_attr.ino);
            assert_eq!(remaining_attr.nlink, 1);

            let remaining_data = cli.read_at(&hard_path, 0, 7).await.unwrap();
            assert_eq!(remaining_data, b"payload".to_vec());

            cli.unlink(&hard_path).await.unwrap();
            cli.rmdir("/links/sub").await.unwrap();
        } else {
            cli.unlink("/links/original.txt").await.unwrap();
        }
        cli.rmdir("/links").await.unwrap();
    }
}
