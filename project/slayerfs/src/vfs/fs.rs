//! FUSE/SDK-friendly VFS with path-based create/mkdir/read/write/readdir/stat support.

use crate::chuck::chunk::ChunkLayout;
use crate::chuck::reader::ChunkReader;
use crate::chuck::store::BlockStore;
use crate::chuck::util::{ChunkSpan, split_file_range_into_chunks};
use crate::chuck::writer::ChunkWriter;
use crate::meta::MetaStore;

// Re-export types from meta::store for convenience
pub use crate::meta::store::{DirEntry, FileAttr, FileType};

#[allow(unused)]
#[allow(clippy::upper_case_acronyms)]
pub struct VFS<S: BlockStore, M: MetaStore> {
    layout: ChunkLayout,
    store: S,
    meta: M,
    base: u64,
    root: i64,
}

#[allow(dead_code)]
impl<S: BlockStore, M: MetaStore> VFS<S, M> {
    pub fn root_ino(&self) -> i64 {
        self.root
    }

    /// get the node's parent inode.
    pub async fn parent_of(&self, ino: i64) -> Option<i64> {
        self.meta.get_parent(ino).await.ok().flatten()
    }

    /// get the node's fullpath.
    pub async fn path_of(&self, ino: i64) -> Option<String> {
        self.meta.get_path(ino).await.ok().flatten()
    }

    /// get the node's child inode by name.
    pub async fn child_of(&self, parent: i64, name: &str) -> Option<i64> {
        self.meta.lookup(parent, name).await.ok().flatten()
    }

    pub async fn stat_ino(&self, ino: i64) -> Option<FileAttr> {
        let meta_attr = self.meta.stat(ino).await.ok().flatten()?;
        Some(meta_attr)
    }

    /// List directory entries by inode
    pub async fn readdir_ino(&self, ino: i64) -> Option<Vec<DirEntry>> {
        let meta_entries = self.meta.readdir(ino).await.ok()?;

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

    pub async fn new(layout: ChunkLayout, store: S, meta: M) -> Result<Self, String> {
        meta.initialize().await.map_err(|e| e.to_string())?;

        let root_ino = meta.root_ino();
        let base = 1_000_000_000u64;

        Ok(Self {
            layout,
            store,
            meta,
            base,
            root: root_ino,
        })
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

    fn chunk_id_for(&self, ino: i64, chunk_index: u64) -> u64 {
        let ino_u64 = u64::try_from(ino).expect("inode must be non-negative");
        ino_u64
            .checked_mul(self.base)
            .and_then(|v| v.checked_add(chunk_index))
            .expect("chunk_id overflow")
    }

    /// Recursively create directories (mkdir -p behavior).
    /// - If an intermediate component exists as a file, return `"not a directory"`.
    /// - Idempotent: existing directories simply return their inode.
    /// - Returns the inode of the target directory.
    pub async fn mkdir_p(&self, path: &str) -> Result<i64, String> {
        let path = Self::norm_path(path);
        if &path == "/" {
            return Ok(self.root);
        }
        if let Ok(Some((ino, _attr))) = self.meta.lookup_path(&path).await {
            return Ok(ino);
        }
        let mut cur_ino = self.root;
        for part in path.trim_start_matches('/').split('/') {
            if part.is_empty() {
                continue;
            }
            match self.meta.lookup(cur_ino, part).await {
                Ok(Some(ino)) => {
                    if let Ok(Some(attr)) = self.meta.stat(ino).await
                        && attr.kind != FileType::Dir
                    {
                        return Err("not a directory".into());
                    }
                    cur_ino = ino;
                }
                _ => {
                    let ino = self
                        .meta
                        .mkdir(cur_ino, part.to_string())
                        .await
                        .map_err(|e| e.to_string())?;
                    cur_ino = ino;
                }
            }
        }
        Ok(cur_ino)
    }

    /// Create a regular file (running `mkdir_p` on its parent if needed).
    /// - If a directory with the same name exists, returns `"is a directory"`.
    /// - If the file already exists, returns its inode instead of creating a new one.
    pub async fn create_file(&self, path: &str) -> Result<i64, String> {
        let path = Self::norm_path(path);
        let (dir, name) = Self::split_dir_file(&path);
        let dir_ino = self.mkdir_p(&dir).await?;

        // check the file exists and then return.
        if let Ok(Some(ino)) = self.meta.lookup(dir_ino, &name).await
            && let Ok(Some(attr)) = self.meta.stat(ino).await
        {
            return if attr.kind == FileType::Dir {
                Err("is a directory".into())
            } else {
                Ok(ino)
            };
        }

        let ino = self
            .meta
            .create_file(dir_ino, name.clone())
            .await
            .map_err(|e| e.to_string())?;
        Ok(ino)
    }

    /// Fetch a file's attributes (kind/size come from the MetaStore); returns None when missing.
    pub async fn stat(&self, path: &str) -> Option<FileAttr> {
        let path = Self::norm_path(path);
        let (ino, _) = self.meta.lookup_path(&path).await.ok()??;
        let meta_attr = self.meta.stat(ino).await.ok().flatten()?;
        Some(meta_attr)
    }

    /// List directory entries; returns None if the path is missing or not a directory.
    /// `.` and `..` are not included.
    pub async fn readdir(&self, path: &str) -> Option<Vec<DirEntry>> {
        let path = Self::norm_path(path);
        let (ino, _) = self.meta.lookup_path(&path).await.ok()??;

        let meta_entries = self.meta.readdir(ino).await.ok()?;

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
        matches!(self.meta.lookup_path(&path).await, Ok(Some(_)))
    }

    /// Remove a regular file (directories are not supported here).
    pub async fn unlink(&self, path: &str) -> Result<(), String> {
        let path = Self::norm_path(path);
        let (dir, name) = Self::split_dir_file(&path);

        let parent_ino = if &dir == "/" {
            self.root
        } else {
            self.meta
                .lookup_path(&dir)
                .await
                .map_err(|e| e.to_string())?
                .ok_or_else(|| "parent not found".to_string())?
                .0
        };

        let ino = self
            .meta
            .lookup(parent_ino, &name)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "not found".to_string())?;

        let attr = self
            .meta
            .stat(ino)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "not found".to_string())?;

        if attr.kind != FileType::File {
            return Err("is a directory".into());
        }

        self.meta
            .unlink(parent_ino, &name)
            .await
            .map_err(|e| e.to_string())?;

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
            self.root
        } else {
            self.meta
                .lookup_path(&dir)
                .await
                .map_err(|e| e.to_string())?
                .ok_or_else(|| "parent not found".to_string())?
                .0
        };

        let ino = self
            .meta
            .lookup(parent_ino, &name)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "not found".to_string())?;

        let attr = self
            .meta
            .stat(ino)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "not found".to_string())?;

        if attr.kind != FileType::Dir {
            return Err("not a directory".into());
        }

        let children = self.meta.readdir(ino).await.map_err(|e| e.to_string())?;
        if !children.is_empty() {
            return Err("directory not empty".into());
        }

        self.meta
            .rmdir(parent_ino, &name)
            .await
            .map_err(|e| e.to_string())?;

        Ok(())
    }

    /// Rename files or directories. The destination must not exist; parent directories are created as needed.
    pub async fn rename(&self, old: &str, new: &str) -> Result<(), String> {
        let old = Self::norm_path(old);
        let new = Self::norm_path(new);
        let (old_dir, old_name) = Self::split_dir_file(&old);
        let (new_dir, new_name) = Self::split_dir_file(&new);

        if self.meta.lookup_path(&new).await.ok().flatten().is_some() {
            return Err("target exists".into());
        }

        let old_parent_ino = if &old_dir == "/" {
            self.root
        } else {
            self.meta
                .lookup_path(&old_dir)
                .await
                .map_err(|e| e.to_string())?
                .ok_or_else(|| "parent not found".to_string())?
                .0
        };

        let new_dir_ino = self.mkdir_p(&new_dir).await?;

        self.meta
            .rename(old_parent_ino, &old_name, new_dir_ino, new_name)
            .await
            .map_err(|e| e.to_string())?;

        Ok(())
    }

    /// Truncate/extend file size (metadata only; holes are read as zeros).
    /// Shrinking does not eagerly reclaim block data.
    pub async fn truncate(&self, path: &str, size: u64) -> Result<(), String> {
        let path = Self::norm_path(path);
        let (ino, _) = self
            .meta
            .lookup_path(&path)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "not found".to_string())?;
        self.meta
            .set_file_size(ino, size)
            .await
            .map_err(|e| e.to_string())
    }

    /// Write data by file offset. Internally splits the range into per-chunk writes.
    /// Writes each affected chunk fragment and updates the size once at the end.
    pub async fn write(&self, path: &str, offset: u64, data: &[u8]) -> Result<usize, String> {
        let path = Self::norm_path(path);
        let (ino, _) = self
            .meta
            .lookup_path(&path)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "not found".to_string())?;

        let spans: Vec<ChunkSpan> = split_file_range_into_chunks(self.layout, offset, data.len());
        let mut cursor = 0usize;
        for sp in spans {
            let cid = self.chunk_id_for(ino, sp.index);
            let writer = ChunkWriter::new(self.layout, cid, &self.store, &self.meta);
            let take = sp.len;
            let buf = &data[cursor..cursor + take];
            writer
                .write(sp.offset as u32, buf)
                .await
                .map_err(|e| e.to_string())?;
            cursor += take;
        }
        // Update size once after writes finish
        let new_size = offset + data.len() as u64;
        self.meta
            .set_file_size(ino, new_size)
            .await
            .map_err(|e| e.to_string())?;
        Ok(data.len())
    }

    /// Read data by file offset.
    /// Read by inode directly
    pub async fn read_ino(&self, ino: i64, offset: u64, len: usize) -> Result<Vec<u8>, String> {
        if len == 0 {
            return Ok(Vec::new());
        }
        let spans: Vec<ChunkSpan> = split_file_range_into_chunks(self.layout, offset, len);
        let mut out = Vec::with_capacity(len);
        for sp in spans {
            let cid = self.chunk_id_for(ino, sp.index);
            let reader = ChunkReader::new(self.layout, cid, &self.store, &self.meta);
            let part = reader
                .read(sp.offset as u32, sp.len)
                .await
                .map_err(|e| e.to_string())?;
            out.extend(part);
        }
        Ok(out)
    }

    /// Read by path (convenience method that uses read_ino internally)
    pub async fn read(&self, path: &str, offset: u64, len: usize) -> Result<Vec<u8>, String> {
        let path = Self::norm_path(path);
        let (ino, _) = self
            .meta
            .lookup_path(&path)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "not found".to_string())?;
        self.read_ino(ino, offset, len).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cadapter::client::ObjectClient;
    use crate::cadapter::localfs::LocalFsBackend;
    use crate::chuck::store::ObjectBlockStore;
    use crate::meta::create_meta_store_from_url;

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
}
