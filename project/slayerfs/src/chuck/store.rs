//! 存储后端抽象：块级读写接口 + 内存实现（异步）。

use super::chunk::ChunkLayout;
use crate::cadapter::client::{ObjectBackend, ObjectClient};
use async_trait::async_trait;
use libc::{KEYCTL_CAPS0_CAPABILITIES, SYS_remap_file_pages};
use moka::{Entry, ops::compute::Op};
use std::{collections::HashMap, io::SeekFrom, path::PathBuf};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

/// 抽象块存储接口（后续可由 cadapter/S3 等实现）。
#[async_trait]
// ensure offset_in_block + data.len() <= block_size
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
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
        }
    }

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

#[derive(Clone, Debug)]
struct CacheItem {
    file_path: PathBuf,
    etag: String,
}

/// 通过 cadapter::client 后端实现的 BlockStore（键空间：chunks/{chunk_id}/{block_index}）。
pub struct ObjectBlockStore<B: ObjectBackend> {
    client: ObjectClient<B>,
    block_cache: moka::future::Cache<String, CacheItem>,
}

impl<B: ObjectBackend> ObjectBlockStore<B> {
    pub fn new(client: ObjectClient<B>) -> Self {
        Self {
            client,
            block_cache: moka::future::Cache::new(10_000),
        }
    }

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
        let existing = self
            .client
            .get_object(&key)
            .await
            .expect("object store get failed");
        let mut buf = existing.unwrap_or_else(|| vec![0u8; bs]);
        if buf.len() < bs {
            buf.resize(bs, 0);
        }
        let start = offset_in_block as usize;
        let end = start + data.len();
        buf[start..end].copy_from_slice(data);
        self.client
            .put_object(&key, &buf)
            .await
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
        let start = offset_in_block as usize;
        let end = start + len;
        let mut buf = vec![0u8; len];
        let entry = self.block_cache.entry(key.clone());
        entry
            .and_compute_with(async |item| match item {
                Some(e) => {
                    let local_etag = e.value().etag.clone();
                    let remote_etag = self.client.get_etag(&key).await.unwrap();
                    let file_path_str = e.value().file_path.clone();
                    let file_path = dirs::cache_dir().unwrap().join(file_path_str);
                    let mut file = tokio::fs::File::open(file_path.clone()).await.unwrap();
                    if local_etag == remote_etag {
                        file.read_exact(&mut buf).await.unwrap();
                    } else {
                        let file_path = dirs::cache_dir().unwrap().join(key.clone());
                        let object = self
                            .client
                            .get_object(&key)
                            .await
                            .unwrap()
                            .expect("failed to read block form s3");
                        buf.copy_from_slice(&object[start..end]);
                        tokio::spawn(async move {
                            let mut open_option = tokio::fs::OpenOptions::new();
                            let mut file = open_option
                                .write(true)
                                .create(true)
                                .open(file_path)
                                .await
                                .unwrap();
                            let _ = file.seek(SeekFrom::Start(0)).await;
                            let _ = file.write_all(&object).await;
                        });
                    }
                    Op::Put(CacheItem {
                        file_path,
                        etag: remote_etag,
                    })
                }
                None => {
                    let file_path = dirs::cache_dir().unwrap().join(key.clone());
                    let object = self
                        .client
                        .get_object(&key)
                        .await
                        .unwrap()
                        .expect("failed to read block form s3");
                    let etag = self.client.get_etag(&key).await.unwrap();
                    buf.copy_from_slice(&object[start..end]);
                    let file_path_clone = file_path.clone();
                    tokio::spawn(async move {
                        let mut open_option = tokio::fs::OpenOptions::new();
                        let mut file = open_option
                            .write(true)
                            .create(true)
                            .open(file_path_clone)
                            .await
                            .unwrap();
                        let _ = file.seek(SeekFrom::Start(0)).await;
                        let _ = file.write_all(&object).await;
                    });
                    Op::Put(CacheItem { file_path, etag })
                }
            })
            .await;
        let _ = layout;
        buf
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
        store
            .write_block_range(42, 3, layout.block_size / 4, &data, layout)
            .await;

        let out = store
            .read_block_range(42, 3, layout.block_size / 4, data.len(), layout)
            .await;
        assert_eq!(out, data);
    }
}
