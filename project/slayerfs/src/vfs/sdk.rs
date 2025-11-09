//! SDK interface: simplified filesystem APIs for applications/SDKs (JuiceFS-style).
//!
//! Goals:
//! - Path-level APIs: mkdir_p/create/read/write/readdir/stat
//! - Pluggable backend: reuse Fs-level BlockStore and MetaStore
//! - Provide a convenient LocalFs constructor

use crate::chuck::chunk::ChunkLayout;
use crate::chuck::store::BlockStore;
use crate::meta::{MetaStore, create_meta_store_from_url};
use crate::vfs::fs::{DirEntry, FileAttr, VFS};

/// SDK client parametrized by its backend.
pub struct Client<S: BlockStore, M: MetaStore> {
    fs: VFS<S, M>,
}

#[allow(unused)]
impl<S: BlockStore, M: MetaStore> Client<S, M> {
    pub async fn new(layout: ChunkLayout, store: S, meta: M) -> Result<Self, String> {
        let fs = VFS::new(layout, store, meta).await?;
        Ok(Self { fs })
    }

    pub async fn mkdir_p(&self, path: &str) -> Result<(), String> {
        let _ = self.fs.mkdir_p(path).await?;
        Ok(())
    }

    pub async fn create(&self, path: &str) -> Result<(), String> {
        let _ = self.fs.create_file(path).await?;
        Ok(())
    }

    pub async fn write_at(
        &mut self,
        path: &str,
        offset: u64,
        data: &[u8],
    ) -> Result<usize, String> {
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

    // Extra helpers: delete / rename / truncate
    pub async fn unlink(&self, path: &str) -> Result<(), String> {
        self.fs.unlink(path).await
    }

    pub async fn rmdir(&self, path: &str) -> Result<(), String> {
        self.fs.rmdir(path).await
    }

    pub async fn rename(&self, old: &str, new: &str) -> Result<(), String> {
        self.fs.rename(old, new).await
    }

    pub async fn truncate(&self, path: &str, size: u64) -> Result<(), String> {
        self.fs.truncate(path, size).await
    }
}

// ============== Convenience builder (LocalFs backend) ==============

use crate::cadapter::client::ObjectClient;
use crate::cadapter::localfs::LocalFsBackend;
use crate::chuck::store::ObjectBlockStore;
use std::path::Path;
use std::sync::Arc;

#[allow(dead_code)]
pub type LocalClient = Client<ObjectBlockStore<LocalFsBackend>, Arc<dyn MetaStore>>;

#[allow(dead_code)]
impl LocalClient {
    #[allow(dead_code)]
    pub async fn new_local<P: AsRef<Path>>(root: P, layout: ChunkLayout) -> Self {
        let client = ObjectClient::new(LocalFsBackend::new(root));
        let store = ObjectBlockStore::new(client);

        let meta = create_meta_store_from_url("sqlite::memory:")
            .await
            .expect("Failed to create meta store");

        let fs = VFS::new(layout, store, meta)
            .await
            .expect("Failed to create VFS");
        Client { fs }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_sdk_local_basic() {
        let layout = ChunkLayout::default();
        let tmp = tempdir().unwrap();
        let mut cli = LocalClient::new_local(tmp.path(), layout).await;

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
        let cli = LocalClient::new_local(tmp.path(), layout).await;

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
}
