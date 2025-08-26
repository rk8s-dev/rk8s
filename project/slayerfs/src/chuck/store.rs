//! 存储后端抽象：块级读写接口 + 内存实现（异步）。

use super::chunk::ChunkLayout;
use crate::cadapter::client::{ObjectBackend, ObjectClient};
use std::collections::HashMap;
use async_trait::async_trait;

/// 抽象块存储接口（后续可由 cadapter/S3 等实现）。
#[async_trait]
pub trait BlockStore {
    async fn write_block_range(
        &mut self,
        chunk_id: i64,
        block_index: u32,
        offset_in_block: u32,
        data: &[u8],
        layout: ChunkLayout,
    );

    async fn read_block_range(
        &self,
        chunk_id: i64,
        block_index: u32,
        offset_in_block: u32,
        len: usize,
        layout: ChunkLayout,
    ) -> Vec<u8>;
}

type BlockKey = (i64 /*chunk_id*/, u32 /*block_index*/);

/// 简单内存实现：用于本地开发/测试。
#[derive(Default)]
pub struct InMemoryBlockStore {
    map: HashMap<BlockKey, Vec<u8>>, // 每个块固定大小
}

impl InMemoryBlockStore {
    pub fn new() -> Self { Self { map: HashMap::new() } }

    fn ensure_block(&mut self, key: BlockKey, block_size: usize) -> &mut Vec<u8> {
        let entry = self.map.entry(key).or_insert_with(|| vec![0u8; block_size]);
        if entry.len() < block_size {
            entry.resize(block_size, 0);
        }
        entry
    }
}

#[async_trait]
impl BlockStore for InMemoryBlockStore {
    async fn write_block_range(
        &mut self,
        chunk_id: i64,
        block_index: u32,
        offset_in_block: u32,
        data: &[u8],
        layout: ChunkLayout,
    ) {
        let block_size = layout.block_size as usize;
        let buf = self.ensure_block((chunk_id, block_index), block_size);
        let start = offset_in_block as usize;
        let end = start + data.len();
        debug_assert!(end <= block_size, "write exceeds block boundary");
        buf[start..end].copy_from_slice(data);
    }

    async fn read_block_range(
        &self,
        chunk_id: i64,
        block_index: u32,
        offset_in_block: u32,
        len: usize,
        layout: ChunkLayout,
    ) -> Vec<u8> {
        let start = offset_in_block as usize;
        let end = start + len;
        if let Some(buf) = self.map.get(&(chunk_id, block_index)) {
            let mut out = vec![0u8; len];
            let copy_end = end.min(buf.len());
            if copy_end > start {
                out[..(copy_end - start)].copy_from_slice(&buf[start..copy_end]);
            }
            out
        } else {
            // 未写入的洞返回 0
            let _ = layout; // 抑制未使用
            vec![0u8; len]
        }
    }
}

/// 通过 cadapter::client 后端实现的 BlockStore（键空间：chunks/{chunk_id}/{block_index}）。
pub struct ObjectBlockStore<B: ObjectBackend> {
    client: ObjectClient<B>,
}

impl<B: ObjectBackend> ObjectBlockStore<B> {
    pub fn new(client: ObjectClient<B>) -> Self { Self { client } }

    fn key_for(chunk_id: i64, block_index: u32) -> String {
        format!("chunks/{chunk_id}/{block_index}")
    }
}

#[async_trait]
impl<B: ObjectBackend + Send + Sync> BlockStore for ObjectBlockStore<B> {
    async fn write_block_range(
        &mut self,
        chunk_id: i64,
        block_index: u32,
        offset_in_block: u32,
        data: &[u8],
        layout: ChunkLayout,
    ) {
        // 读取已有对象（若存在），在内存拼接后整体写回；MVP 简化。
        let key = Self::key_for(chunk_id, block_index);
        let bs = layout.block_size as usize;
        // 失败直接 panic，与原同步实现行为一致；后续可改为返回 Result。
        let existing = self.client.get_object(&key).await
            .expect("object store get failed");
        let mut buf = existing.unwrap_or_else(|| vec![0u8; bs]);
        if buf.len() < bs { buf.resize(bs, 0); }
        let start = offset_in_block as usize;
        let end = start + data.len();
        buf[start..end].copy_from_slice(data);
        self.client.put_object(&key, &buf).await
            .expect("object store put failed");
    }

    async fn read_block_range(
        &self,
        chunk_id: i64,
        block_index: u32,
        offset_in_block: u32,
        len: usize,
        layout: ChunkLayout,
    ) -> Vec<u8> {
        let key = Self::key_for(chunk_id, block_index);
        let data = self.client.get_object(&key).await.ok().flatten();
        if let Some(buf) = data {
            let start = offset_in_block as usize;
            let end = (start + len).min(buf.len());
            let mut out = vec![0u8; len];
            if end > start {
                out[..(end - start)].copy_from_slice(&buf[start..end]);
            }
            out
        } else {
            let _ = layout; // 抑制未使用
            vec![0u8; len]
        }
    }
}

/// 便捷别名：基于真实 S3Backend 的 BlockStore
#[allow(dead_code)]
pub type S3BlockStore = ObjectBlockStore<crate::cadapter::s3::S3Backend>;
/// 便捷别名：基于 RustfsLikeBackend 的 BlockStore
#[allow(dead_code)]
pub type RustfsBlockStore = ObjectBlockStore<crate::cadapter::rustfs::RustfsLikeBackend>;
/// 便捷别名：基于 LocalFsBackend 的 BlockStore（mock 本地目录）
#[allow(dead_code)]
pub type LocalFsBlockStore = ObjectBlockStore<crate::cadapter::localfs::LocalFsBackend>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cadapter::client::ObjectClient;
    use crate::cadapter::localfs::LocalFsBackend;
    use crate::chuck::chunk::ChunkLayout;

    #[tokio::test]
    async fn test_localfs_block_store_put_get() {
        let tmp = tempfile::tempdir().unwrap();
        let client = ObjectClient::new(LocalFsBackend::new(tmp.path()));
        let mut store = ObjectBlockStore::new(client);
        let layout = ChunkLayout::default();

        let data = vec![7u8; layout.block_size as usize / 2];
        store.write_block_range(42, 3, layout.block_size / 4, &data, layout).await;

        let out = store.read_block_range(42, 3, layout.block_size / 4, data.len(), layout).await;
        assert_eq!(out, data);
    }
}
