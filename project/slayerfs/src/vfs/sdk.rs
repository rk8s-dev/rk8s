//! SDK interface: simplified filesystem APIs for applications/SDKs (JuiceFS-style).
//!
//! Goals:
//! - Path-level APIs: mkdir_p/create/read/write/readdir/stat
//! - Pluggable backend: reuse Fs-level BlockStore and metadata layer
//! - Provide a convenient LocalFs constructor

use crate::chuck::chunk::ChunkLayout;
use crate::chuck::store::BlockStore;
use crate::meta::MetaLayer;
use crate::meta::client::MetaClient;
use crate::meta::factory::create_meta_store_from_url;
use crate::meta::file_lock::{FileLockInfo, FileLockQuery, FileLockRange, FileLockType};
use crate::vfs::error::{PathHint, VfsError};
use crate::vfs::fs::{DirEntry, FileAttr, FileType, VFS};
use std::path::Path;
use std::sync::Arc;

/// SDK client parametrized by its backend.
pub struct Client<S: BlockStore + Send + Sync + 'static, M: MetaLayer + Send + Sync + 'static> {
    fs: VFS<S, M>,
}

#[allow(unused)]
impl<S: BlockStore + Send + Sync + 'static, M: MetaLayer + Send + Sync + 'static> Client<S, M> {
    pub async fn new(layout: ChunkLayout, store: S, meta_layer: Arc<M>) -> Result<Self, VfsError> {
        let fs = VFS::with_meta_layer(layout, store, meta_layer)?;
        Ok(Self { fs })
    }
}

#[allow(unused)]
impl<S: BlockStore + Send + Sync + 'static, M: MetaLayer + Send + Sync + 'static> Client<S, M> {
    pub fn from_vfs(fs: VFS<S, M>) -> Self {
        Self { fs }
    }

    pub async fn mkdir_p(&self, path: &str) -> Result<(), VfsError> {
        let _ = self.fs.mkdir_p(path).await?;
        Ok(())
    }

    pub async fn create(&self, path: &str) -> Result<(), VfsError> {
        let _ = self.fs.create_file(path).await?;
        Ok(())
    }

    pub async fn write_at(
        &mut self,
        path: &str,
        offset: u64,
        data: &[u8],
    ) -> Result<usize, VfsError> {
        let attr = self.fs.stat(path).await?;
        let fh = self.fs.open(attr.ino, attr, false, true).await?;
        let result = self.fs.write(fh, offset, data).await;
        let _ = self.fs.close(fh).await;
        result
    }

    pub async fn read_at(&self, path: &str, offset: u64, len: usize) -> Result<Vec<u8>, VfsError> {
        let attr = self.fs.stat(path).await?;
        let fh = self.fs.open(attr.ino, attr, true, false).await?;
        let result = self.fs.read(fh, offset, len).await;
        let _ = self.fs.close(fh).await;
        result
    }

    pub async fn readdir(&self, path: &str) -> Result<Vec<DirEntry>, VfsError> {
        let attr = self.fs.stat(path).await?;
        if attr.kind != FileType::Dir {
            return Err(VfsError::NotADirectory {
                path: PathHint::some(path),
            });
        }
        let fh = self.fs.opendir(attr.ino).await?;
        let mut offset = 0u64;
        let mut entries = Vec::new();
        loop {
            let batch = self.fs.readdir(fh, offset).unwrap_or_default();
            if batch.is_empty() {
                break;
            }
            offset += batch.len() as u64;
            entries.extend(batch);
        }
        let _ = self.fs.closedir(fh);
        Ok(entries)
    }

    pub async fn stat(&self, path: &str) -> Result<FileAttr, VfsError> {
        self.fs.stat(path).await
    }

    pub async fn link(&self, existing: &str, link_path: &str) -> Result<FileAttr, VfsError> {
        self.fs.link(existing, link_path).await
    }

    pub async fn symlink(&self, link_path: &str, target: &str) -> Result<FileAttr, VfsError> {
        let (_, attr) = self.fs.create_symlink(link_path, target).await?;
        Ok(attr)
    }

    pub async fn readlink(&self, path: &str) -> Result<String, VfsError> {
        self.fs.readlink(path).await
    }

    // Extra helpers: delete / rename / truncate
    pub async fn unlink(&self, path: &str) -> Result<(), VfsError> {
        self.fs.unlink(path).await
    }

    pub async fn rmdir(&self, path: &str) -> Result<(), VfsError> {
        self.fs.rmdir(path).await
    }

    pub async fn rename(&self, old: &str, new: &str) -> Result<(), VfsError> {
        self.fs.rename(old, new).await
    }

    pub async fn truncate(&self, path: &str, size: u64) -> Result<(), VfsError> {
        self.fs.truncate(path, size).await
    }

    /// Get file lock information for a given path and query.
    pub async fn get_plock(
        &self,
        path: &str,
        query: &FileLockQuery,
    ) -> Result<FileLockInfo, VfsError> {
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
    ) -> Result<(), VfsError> {
        self.fs
            .set_plock(path, owner, block, lock_type, range, pid)
            .await
    }
}

// ============== Convenience builder (LocalFs backend) ==============

use crate::cadapter::client::ObjectClient;
use crate::cadapter::localfs::LocalFsBackend;
use crate::chuck::store::ObjectBlockStore;
use crate::meta::stores::database_store::DatabaseMetaStore;

#[allow(dead_code)]
pub type LocalClient = Client<ObjectBlockStore<LocalFsBackend>, MetaClient<DatabaseMetaStore>>;

#[allow(dead_code)]
impl LocalClient {
    #[allow(dead_code)]
    pub async fn new_local<P: AsRef<Path>>(root: P, layout: ChunkLayout) -> Result<Self, VfsError> {
        let client = ObjectClient::new(LocalFsBackend::new(root));
        let meta_handle = create_meta_store_from_url("sqlite::memory:").await?;
        let meta_layer = meta_handle.layer();
        let store = ObjectBlockStore::new(client);
        let fs = VFS::with_meta_layer(layout, store, meta_layer)?;
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
        let mut cli = LocalClient::new_local(tmp.path(), layout)
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
        let mut cli = LocalClient::new_local(tmp.path(), layout)
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
                matches!(err, VfsError::Unsupported | VfsError::Other),
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
