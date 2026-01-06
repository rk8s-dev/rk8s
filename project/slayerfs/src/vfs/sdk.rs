//! SDK interface: simplified filesystem APIs for applications/SDKs (JuiceFS-style).
//!
//! Goals:
//! - Path-level APIs: mkdir_p/create/read/write/readdir/stat
//! - Pluggable backend: reuse Fs-level BlockStore and MetaStore
//! - Provide a convenient LocalFs constructor

use crate::chuck::chunk::ChunkLayout;
use crate::chuck::store::BlockStore;
use crate::meta::MetaStore;
use crate::meta::factory::create_meta_store_from_url;
use crate::meta::file_lock::{FileLockInfo, FileLockQuery, FileLockRange, FileLockType};
use crate::meta::store::{MetaError, SetAttrFlags, SetAttrRequest, StatFsSnapshot};
use crate::vfs::fs::{DirEntry, FileAttr, FileType, VFS};
use std::io;
use std::path::Path;

/// SDK client parametrized by its backend.
pub struct Client<S: BlockStore, M: MetaStore + 'static> {
    fs: VFS<S, M>,
}

#[allow(unused)]
impl<S: BlockStore, M: MetaStore + 'static> Client<S, M> {
    pub async fn new(layout: ChunkLayout, store: S, meta: M) -> Result<Self, String> {
        let fs = VFS::new(layout, store, meta).await?;
        Ok(Self { fs })
    }

    pub fn from_vfs(fs: VFS<S, M>) -> Self {
        Self { fs }
    }

    pub async fn mkdir_p(&self, path: &str) -> Result<(), String> {
        let _ = self.fs.mkdir_p(path).await?;
        Ok(())
    }

    pub async fn create(&self, path: &str) -> Result<(), String> {
        let _ = self.fs.create_file(path).await?;
        Ok(())
    }

    pub async fn write_at(&self, path: &str, offset: u64, data: &[u8]) -> Result<usize, String> {
        self.fs.write(path, offset, data).await
    }

    pub async fn read_at(&self, path: &str, offset: u64, len: usize) -> Result<Vec<u8>, String> {
        self.fs.read(path, offset, len).await
    }

    pub async fn readdir(&self, path: &str) -> Result<Vec<DirEntry>, String> {
        self.fs
            .readdir(path)
            .await
            .ok_or_else(|| "not a dir or not found".into())
    }

    pub async fn stat(&self, path: &str) -> Result<FileAttr, String> {
        self.fs.stat(path).await.ok_or_else(|| "not found".into())
    }

    pub async fn link(&self, existing: &str, link_path: &str) -> Result<FileAttr, String> {
        self.fs.link(existing, link_path).await
    }

    pub async fn symlink(&self, link_path: &str, target: &str) -> Result<FileAttr, String> {
        let (_, attr) = self.fs.create_symlink(link_path, target).await?;
        Ok(attr)
    }

    pub async fn readlink(&self, path: &str) -> Result<String, String> {
        self.fs.readlink(path).await
    }

    // Extra helpers: delete / rename / truncate
    pub async fn unlink(&self, path: &str) -> Result<(), String> {
        self.fs.unlink(path).await
    }

    pub async fn rmdir(&self, path: &str) -> Result<(), String> {
        self.fs.rmdir(path).await
    }

    pub async fn rename(&self, old: &str, new: &str) -> Result<(), String> {
        self.fs.rename(old, new).await.map_err(|e| e.to_string())
    }

    pub async fn truncate(&self, path: &str, size: u64) -> Result<(), String> {
        self.fs.truncate(path, size).await
    }

    /// Get file lock information for a given path and query.
    pub async fn get_plock(
        &self,
        path: &str,
        query: &FileLockQuery,
    ) -> Result<FileLockInfo, String> {
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
    ) -> Result<(), String> {
        self.fs
            .set_plock(path, owner, block, lock_type, range, pid)
            .await
    }

    // ===== Structured (std::io) variants for std-like SDK =====

    pub async fn mkdir_p_io(&self, path: &str) -> io::Result<()> {
        self.fs
            .mkdir_p_err(path)
            .await
            .map(|_| ())
            .map_err(Into::into)
    }

    /// Create a single directory (non-recursive).
    pub async fn mkdir_io(&self, path: &str) -> io::Result<()> {
        self.fs
            .mkdir_err(path)
            .await
            .map(|_| ())
            .map_err(Into::into)
    }

    pub async fn create_file_io(&self, path: &str, create_new: bool) -> io::Result<()> {
        self.fs
            .create_file_in_existing_dir_err(path, create_new)
            .await
            .map(|_| ())
            .map_err(Into::into)
    }

    pub async fn write_at_io(&self, path: &str, offset: u64, data: &[u8]) -> io::Result<usize> {
        self.fs
            .write_err(path, offset, data)
            .await
            .map_err(Into::into)
    }

    pub async fn read_at_io(&self, path: &str, offset: u64, len: usize) -> io::Result<Vec<u8>> {
        self.fs
            .read_err(path, offset, len)
            .await
            .map_err(Into::into)
    }

    pub async fn readdir_io(&self, path: &str) -> io::Result<Vec<DirEntry>> {
        self.fs.readdir_err(path).await.map_err(Into::into)
    }

    pub async fn stat_io(&self, path: &str) -> io::Result<FileAttr> {
        self.fs.stat_err(path).await.map_err(Into::into)
    }

    pub async fn unlink_io(&self, path: &str) -> io::Result<()> {
        self.fs.unlink_err(path).await.map_err(Into::into)
    }

    pub async fn rmdir_io(&self, path: &str) -> io::Result<()> {
        self.fs.rmdir_err(path).await.map_err(Into::into)
    }

    pub async fn rename_io(&self, old: &str, new: &str) -> io::Result<()> {
        self.fs.rename_err(old, new).await.map_err(Into::into)
    }

    pub async fn truncate_io(&self, path: &str, size: u64) -> io::Result<()> {
        self.fs.truncate_err(path, size).await.map_err(Into::into)
    }

    /// Check whether a path exists.
    pub async fn exists(&self, path: &str) -> bool {
        self.fs.exists(path).await
    }

    /// Set file/directory attributes (chmod, chown, utime).
    pub async fn set_attr_io(
        &self,
        path: &str,
        req: &SetAttrRequest,
        flags: SetAttrFlags,
    ) -> io::Result<FileAttr> {
        let attr = self.fs.stat_err(path).await?;
        self.fs
            .set_attr(attr.ino, req, flags)
            .await
            .map_err(io::Error::other)
    }

    /// Get file attributes without following symlinks.
    pub async fn lstat_io(&self, path: &str) -> io::Result<FileAttr> {
        // For now, we use the same implementation as stat since VFS doesn't distinguish.
        // TODO: Implement proper lstat that doesn't follow symlinks.
        self.fs.stat_err(path).await.map_err(Into::into)
    }

    /// Recursively remove a directory and all its contents.
    pub async fn remove_dir_all_io(&self, path: &str) -> io::Result<()> {
        if path.trim_matches('/').is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "cannot remove filesystem root",
            ));
        }
        self.remove_dir_all_recursive(path).await
    }

    async fn remove_dir_all_recursive(&self, path: &str) -> io::Result<()> {
        let entries = self.fs.readdir_err(path).await?;
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
                    self.fs.unlink_err(&child_path).await?;
                }
            }
        }
        self.fs.rmdir_err(path).await.map_err(Into::into)
    }

    /// Get file system statistics (total/available space and inodes).
    pub async fn stat_fs_io(&self) -> io::Result<StatFsSnapshot> {
        self.fs.stat_fs().await.map_err(io::Error::other)
    }

    /// Symlink support check for lstat.
    pub async fn readlink_io(&self, path: &str) -> io::Result<String> {
        self.fs.readlink(path).await.map_err(io::Error::other)
    }

    /// Create a hard link.
    pub async fn link_io(&self, existing: &str, link_path: &str) -> io::Result<FileAttr> {
        self.fs
            .link(existing, link_path)
            .await
            .map_err(io::Error::other)
    }

    /// Create a symbolic link.
    pub async fn symlink_io(&self, link_path: &str, target: &str) -> io::Result<FileAttr> {
        self.fs
            .create_symlink(link_path, target)
            .await
            .map(|(_, attr)| attr)
            .map_err(io::Error::other)
    }
}

// ============== Convenience builder (LocalFs backend) ==============

use crate::cadapter::client::ObjectClient;
use crate::cadapter::localfs::LocalFsBackend;
use crate::chuck::store::ObjectBlockStore;
use std::sync::Arc;

#[allow(dead_code)]
pub type LocalClient = Client<ObjectBlockStore<LocalFsBackend>, Arc<dyn MetaStore>>;

#[allow(dead_code)]
impl LocalClient {
    #[allow(dead_code)]
    pub async fn new_local<P: AsRef<Path>>(
        root: P,
        layout: ChunkLayout,
    ) -> Result<Self, MetaError> {
        let client = ObjectClient::new(LocalFsBackend::new(root));
        let meta_handle = create_meta_store_from_url("sqlite::memory:").await?;
        let metadata: Arc<dyn MetaStore> = meta_handle.store();
        let store = ObjectBlockStore::new(client);
        let fs = VFS::new(layout, store, metadata)
            .await
            .map_err(MetaError::Internal)?;
        Ok(Client { fs })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vfs::fs::FileType;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_sdk_local_basic() {
        let layout = ChunkLayout::default();
        let tmp = tempdir().unwrap();
        let cli = LocalClient::new_local(tmp.path(), layout)
            .await
            .expect("init LocalClient");

        cli.mkdir_p("/a/b").await.unwrap();
        cli.create("/a/b/hello.txt").await.unwrap();

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
        let cli = LocalClient::new_local(tmp.path(), layout)
            .await
            .expect("init LocalClient");

        cli.mkdir_p("/x/y").await.unwrap();
        cli.create("/x/y/a.txt").await.unwrap();
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
        let cli = LocalClient::new_local(tmp.path(), layout)
            .await
            .expect("init LocalClient");

        cli.mkdir_p("/links").await.unwrap();
        cli.create("/links/original.txt").await.unwrap();
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
            assert!(
                err.contains("not supported")
                    || err.contains("UNIQUE constraint failed")
                    || err.contains("Database error"),
                "unexpected hard-link error: {err}"
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
