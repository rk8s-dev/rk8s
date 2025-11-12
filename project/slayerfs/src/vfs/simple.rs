//! Minimal VFS: provides the smallest open/write/read workflow for FUSE or SDK integration.

use crate::chuck::chunk::ChunkLayout;
use crate::chuck::reader::ChunkReader;
use crate::chuck::store::BlockStore;
use crate::chuck::writer::ChunkWriter;
use crate::meta::MetaStore;
use std::convert::TryInto;

/// Minimal VFS object that holds the block store and metadata store.
pub struct SimpleVfs<S: BlockStore, M: MetaStore> {
    layout: ChunkLayout,
    store: S,
    meta: M,
}

#[allow(unused)]
impl<S: BlockStore, M: MetaStore> SimpleVfs<S, M> {
    pub fn new(layout: ChunkLayout, store: S, meta: M) -> Self {
        Self {
            layout,
            store,
            meta,
        }
    }

    /// Create a file and return its inode.
    pub async fn create(&mut self, filename: String) -> Result<i64, String> {
        let root_ino = self.meta.root_ino();
        let ino = self
            .meta
            .create_file(root_ino, filename)
            .await
            .map_err(|e| e.to_string())?;
        Ok(ino)
    }

    /// Write data into a specific chunk (offset is chunk-local).
    pub async fn pwrite_chunk(
        &mut self,
        ino: i64,
        chunk_id: u64,
        off_in_chunk: u64,
        data: &[u8],
    ) -> Result<(), String> {
        // Perform the write first
        let writer = ChunkWriter::new(self.layout, chunk_id, &self.store, &self.meta);
        writer
            .write(
                off_in_chunk
                    .try_into()
                    .expect("chunk offset must fit in u32"),
                data,
            )
            .await
            .map_err(|e| e.to_string())?;

        // Simplified sizing: approximate the file size with chunk-local offset
        let new_size = off_in_chunk + data.len() as u64;
        self.meta
            .set_file_size(ino, new_size)
            .await
            .map_err(|e| e.to_string())?;

        Ok(())
    }

    /// Read data from a specific chunk (offset is chunk-local).
    pub async fn pread_chunk(
        &self,
        _ino: i64,
        chunk_id: u64,
        off_in_chunk: u64,
        len: usize,
    ) -> Result<Vec<u8>, String> {
        let mut reader = ChunkReader::new(self.layout, chunk_id, &self.store, &self.meta);
        reader.prepare_slices().await.map_err(|e| e.to_string())?;
        reader
            .read(
                off_in_chunk
                    .try_into()
                    .expect("chunk offset must fit in u32"),
                len,
            )
            .await
            .map_err(|e| e.to_string())
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
    async fn test_simple_vfs_write_read() {
        let layout = ChunkLayout::default();
        let tmp = tempfile::tempdir().unwrap();
        let client = ObjectClient::new(LocalFsBackend::new(tmp.path()));
        let store = ObjectBlockStore::new(client);

        let meta = create_meta_store_from_url("sqlite::memory:").await.unwrap();
        let mut vfs = SimpleVfs::new(layout, store, meta);

        let ino = vfs.create("test_file.txt".to_string()).await.unwrap();
        let chunk_id = 1u64;
        let half = (layout.block_size / 2) as usize;
        let len = layout.block_size as usize + half;
        let mut data = vec![0u8; len];
        for (i, b) in data.iter_mut().enumerate().take(len) {
            *b = (i % 251) as u8;
        }
        vfs.pwrite_chunk(ino, chunk_id, half as u64, &data)
            .await
            .unwrap();
        let out = vfs
            .pread_chunk(ino, chunk_id, half as u64, len)
            .await
            .unwrap();
        assert_eq!(out, data);
    }
}
