//! Common test utilities for SlayerFS tests
#![allow(dead_code)]

use slayerfs::chunk::{
    ChunkLayout,
    slice::{SliceDesc, SliceOffset, block_span_iter_slice},
    store::BlockStore,
};
use slayerfs::meta::SLICE_ID_KEY;
use slayerfs::meta::store::MetaStore;
use slayerfs::{Config, DatabaseMetaStore, DatabaseType, InMemoryBlockStore};
use std::sync::Arc;
use tempfile::TempDir;

/// Setup a test filesystem with SQLite backend
pub async fn setup_test_fs() -> (TempDir, Arc<DatabaseMetaStore>, Arc<InMemoryBlockStore>) {
    let tmp_dir = TempDir::new().unwrap();
    let db_path = tmp_dir.path().join("test.db");

    let config = Config {
        database: slayerfs::DatabaseConfig {
            db_config: DatabaseType::Sqlite {
                url: format!("sqlite://{}?mode=rwc", db_path.display()),
            },
        },
        cache: Default::default(),
        client: Default::default(),
        compact: Default::default(),
    };

    let meta_store = Arc::new(DatabaseMetaStore::from_config(config).await.unwrap());
    let block_store = Arc::new(InMemoryBlockStore::new());

    meta_store.initialize().await.unwrap();

    (tmp_dir, meta_store, block_store)
}

/// Generate a test pattern of bytes
pub fn generate_test_pattern(size: usize) -> Vec<u8> {
    (0..size).map(|i| (i % 256) as u8).collect()
}

/// Create a fragmented file with overlapping writes
pub async fn create_fragmented_file(
    meta_store: &Arc<dyn MetaStore>,
    block_store: &Arc<InMemoryBlockStore>,
    ino: i64,
    num_overlapping_writes: usize,
) -> u64 {
    use slayerfs::vfs::chunk_id_for;

    let chunk_id = chunk_id_for(ino, 0).unwrap();
    let layout = ChunkLayout::default();

    for i in 0..num_overlapping_writes {
        let offset = (i * 512) as u64;
        let data = vec![(i % 256) as u8; 1024];
        let slice_id = meta_store.next_id(SLICE_ID_KEY).await.unwrap() as u64;
        let slice = SliceDesc {
            slice_id,
            chunk_id,
            offset,
            length: data.len() as u64,
        };

        let spans: Vec<_> =
            block_span_iter_slice(SliceOffset(0), data.len() as u64, layout).collect();
        let mut written = 0usize;
        for span in spans {
            let block_key = (slice_id, span.index as u32);
            let end = (written + span.len as usize).min(data.len());
            block_store
                .write_fresh_range(block_key, span.offset, &data[written..end])
                .await
                .unwrap();
            written = end;
        }

        let current_size = match meta_store.stat(ino).await.unwrap() {
            Some(attr) => attr.size,
            None => 0,
        };
        let new_size = (offset + data.len() as u64).max(current_size);
        meta_store
            .write(ino, chunk_id, slice, new_size)
            .await
            .unwrap();
    }
    chunk_id
}
