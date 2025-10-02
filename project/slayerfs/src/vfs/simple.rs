//! 简化版 VFS：提供最小的 open/write/read 流程，便于对接 FUSE 和 SDK。

use crate::chuck::chunk::ChunkLayout;
use crate::chuck::reader::ChunkReader;
use crate::chuck::store::BlockStore;
use crate::chuck::writer::ChunkWriter;
use crate::meta::MetaStore;

/// 一个最小的 VFS 对象，持有块存储与元数据存储。
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

    /// 创建文件，返回 inode 编号。
    pub async fn create(&mut self, filename: String) -> Result<i64, String> {
        let root_ino = self.meta.root_ino();
        let ino = self
            .meta
            .create_file(root_ino, filename)
            .await
            .map_err(|e| e.to_string())?;
        Ok(ino)
    }

    /// 在指定文件的某个 chunk 上写入（偏移为 chunk 内偏移）。
    pub async fn pwrite_chunk(
        &mut self,
        ino: i64,
        chunk_id: i64,
        off_in_chunk: u64,
        data: &[u8],
    ) -> Result<(), String> {
        // 写数据
        let mut writer = ChunkWriter::new(self.layout, chunk_id, &mut self.store);
        let slice = writer.write(off_in_chunk, data).await;

        // 简化: 使用 chunk 内偏移近似文件大小
        let new_size = off_in_chunk + data.len() as u64;
        self.meta
            .set_file_size(ino, new_size)
            .await
            .map_err(|e| e.to_string())?;

        Ok(())
    }

    /// 在指定文件的某个 chunk 上读取（偏移为 chunk 内偏移）。
    pub async fn pread_chunk(
        &self,
        _ino: i64,
        chunk_id: i64,
        off_in_chunk: u64,
        len: usize,
    ) -> Vec<u8> {
        let reader = ChunkReader::new(self.layout, chunk_id, &self.store);
        reader.read(off_in_chunk, len).await
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
        let chunk_id = 1i64;
        let half = (layout.block_size / 2) as usize;
        let len = layout.block_size as usize + half;
        let mut data = vec![0u8; len];
        for (i, b) in data.iter_mut().enumerate().take(len) {
            *b = (i % 251) as u8;
        }
        vfs.pwrite_chunk(ino, chunk_id, half as u64, &data)
            .await
            .unwrap();
        let out = vfs.pread_chunk(ino, chunk_id, half as u64, len).await;
        assert_eq!(out, data);
    }
}
