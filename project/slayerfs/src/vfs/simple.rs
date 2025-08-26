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

impl<S: BlockStore, M: MetaStore> SimpleVfs<S, M> {
    pub fn new(layout: ChunkLayout, store: S, meta: M) -> Self { Self { layout, store, meta } }

    /// 创建文件，返回 inode 编号。
    pub async fn create(&mut self) -> i64 {
        let mut tx = self.meta.begin().await;
        let ino = tx.alloc_inode().await;
        tx.commit().await.expect("meta commit");
        ino
    }

    /// 在指定文件的某个 chunk 上写入（偏移为 chunk 内偏移）。
    pub async fn pwrite_chunk(&mut self, _ino: i64, chunk_id: i64, off_in_chunk: u64, data: &[u8]) {
        // 写数据
        {
            let mut writer = ChunkWriter::new(self.layout, chunk_id, &mut self.store);
            let slice = writer.write(off_in_chunk, data).await;
            // 记录元数据（本简化实现：记录 slice，并更新 size= max(size, off+len)）。
            let mut tx = self.meta.begin().await;
            tx.record_slice(_ino, slice).await.expect("record slice");
            let new_size = (off_in_chunk + data.len() as u64) as u64; // 简化: 使用 chunk 内偏移近似文件大小
            tx.update_inode_size(_ino, new_size).await.expect("update size");
            tx.commit().await.expect("meta commit");
        }
    }

    /// 在指定文件的某个 chunk 上读取（偏移为 chunk 内偏移）。
    pub async fn pread_chunk(&self, _ino: i64, chunk_id: i64, off_in_chunk: u64, len: usize) -> Vec<u8> {
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
    use crate::meta::InMemoryMetaStore;

    #[tokio::test]
    async fn test_simple_vfs_write_read() {
        let layout = ChunkLayout::default();
        let tmp = tempfile::tempdir().unwrap();
        let client = ObjectClient::new(LocalFsBackend::new(tmp.path()));
        let store = ObjectBlockStore::new(client);
        let meta = InMemoryMetaStore::new();
        let mut vfs = SimpleVfs::new(layout, store, meta);

        let ino = vfs.create().await;
        let chunk_id = 1i64;
        let half = (layout.block_size / 2) as usize;
        let len = layout.block_size as usize + half;
        let mut data = vec![0u8; len];
        for i in 0..len { data[i] = (i % 251) as u8; }
        vfs.pwrite_chunk(ino, chunk_id, half as u64, &data).await;
        let out = vfs.pread_chunk(ino, chunk_id, half as u64, len).await;
        assert_eq!(out, data);
    }
}
