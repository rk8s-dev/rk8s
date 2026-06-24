//! Unit tests for Compactor module
//!
//! These tests focus on the core compaction logic without requiring
//! a full filesystem setup.

mod tests {
    use slayerfs::DatabaseMetaStore;
    use slayerfs::chunk::{
        ChunkLayout, CompactResult, Compactor, slice::SliceDesc, store::InMemoryBlockStore,
    };
    use slayerfs::meta::store::MetaStore;
    use slayerfs::{Config, DatabaseConfig, DatabaseType};
    use std::sync::Arc;
    use tempfile::TempDir;

    async fn setup_test_env() -> (TempDir, Arc<DatabaseMetaStore>, Arc<InMemoryBlockStore>) {
        let tmp_dir = TempDir::new().unwrap();
        let db_path = tmp_dir.path().join("test.db");

        let config = Config {
            database: DatabaseConfig {
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

    #[tokio::test]
    async fn test_should_compact_empty_chunk() {
        let (_tmp, meta_store, block_store) = setup_test_env().await;
        let compactor = Compactor::new(meta_store, block_store);

        // Create a chunk with no slices
        let (should_compact, is_sync) = compactor.should_compact(9999).await.unwrap();

        assert!(!should_compact, "Empty chunk should not trigger compaction");
        assert!(!is_sync, "Empty chunk should not require sync");
    }

    #[tokio::test]
    async fn test_should_compact_single_slice() {
        let (_tmp, meta_store, block_store) = setup_test_env().await;
        let compactor = Compactor::new(meta_store.clone(), block_store);

        // Create a file with single slice
        let root = meta_store.root_ino();
        let file = meta_store
            .create_file(root, "single.txt".to_string())
            .await
            .unwrap();

        // Write single slice
        let chunk_id = slayerfs::vfs::chunk_id_for(file, 0).unwrap();
        let slice = SliceDesc {
            slice_id: 1,
            chunk_id,
            offset: 0,
            length: 4096,
        };
        meta_store.write(file, chunk_id, slice, 4096).await.unwrap();

        let (should_compact, is_sync) = compactor.should_compact(chunk_id).await.unwrap();

        assert!(
            !should_compact,
            "Single slice chunk should not trigger compaction"
        );
        assert!(!is_sync);
    }

    #[tokio::test]
    async fn test_analyze_chunk_fragmentation() {
        let (_tmp, meta_store, block_store) = setup_test_env().await;
        let compactor = Compactor::new(meta_store.clone(), block_store);

        // Create a file with overlapping slices
        let root = meta_store.root_ino();
        let file = meta_store
            .create_file(root, "frag.txt".to_string())
            .await
            .unwrap();
        let chunk_id = slayerfs::vfs::chunk_id_for(file, 0).unwrap();

        // Create overlapping slices to generate fragmentation
        for i in 0..5u64 {
            let slice = SliceDesc {
                slice_id: i + 1,
                chunk_id,
                offset: i * 512, // Overlapping: 0-1024, 512-1536, etc.
                length: 1024,
            };
            meta_store
                .write(file, chunk_id, slice, (i + 1) * 512 + 1024)
                .await
                .unwrap();
        }

        let (count, _total, frag) = compactor.analyze_chunk(chunk_id).await.unwrap();

        assert_eq!(count, 5, "Should have 5 slices");
        assert!(
            frag > 0.0,
            "Fragmentation should be > 0 for overlapping slices"
        );
        println!("Fragmentation: {:.2}", frag);
    }

    #[tokio::test]
    async fn test_compact_light_removes_fully_covered() {
        let (_tmp, meta_store, block_store) = setup_test_env().await;
        let compactor = Compactor::new(meta_store.clone(), block_store);

        let root = meta_store.root_ino();
        let file = meta_store
            .create_file(root, "light.txt".to_string())
            .await
            .unwrap();
        let chunk_id = slayerfs::vfs::chunk_id_for(file, 0).unwrap();

        // Create fully covered scenario:
        // Slice 1: [0, 1000) - will be fully covered
        // Slice 2: [0, 2000) - covers slice 1 completely
        let slice1 = SliceDesc {
            slice_id: 1,
            chunk_id,
            offset: 0,
            length: 1000,
        };
        meta_store
            .write(file, chunk_id, slice1, 1000)
            .await
            .unwrap();

        let slice2 = SliceDesc {
            slice_id: 2,
            chunk_id,
            offset: 0,
            length: 2000,
        };
        meta_store
            .write(file, chunk_id, slice2, 2000)
            .await
            .unwrap();

        let result = compactor.compact_light(chunk_id).await.unwrap();

        assert!(result.is_some(), "Should have removed slices");
        assert_eq!(result.unwrap(), 1, "Should remove 1 fully covered slice");

        // Verify slice count after compaction
        let slices = meta_store.get_slices(chunk_id).await.unwrap();
        assert_eq!(slices.len(), 1, "Should have 1 slice remaining");
        assert_eq!(slices[0].slice_id, 2, "Remaining slice should be #2");
    }

    #[tokio::test]
    async fn test_compact_light_no_change_when_no_full_coverage() {
        let (_tmp, meta_store, block_store) = setup_test_env().await;
        let compactor = Compactor::new(meta_store.clone(), block_store);

        let root = meta_store.root_ino();
        let file = meta_store
            .create_file(root, "no_cover.txt".to_string())
            .await
            .unwrap();
        let chunk_id = slayerfs::vfs::chunk_id_for(file, 0).unwrap();

        // Create partially overlapping slices (no full coverage)
        // Slice 1: [0, 1000)
        // Slice 2: [500, 1500) - overlaps but doesn't fully cover slice 1
        let slice1 = SliceDesc {
            slice_id: 1,
            chunk_id,
            offset: 0,
            length: 1000,
        };
        meta_store
            .write(file, chunk_id, slice1, 1000)
            .await
            .unwrap();

        let slice2 = SliceDesc {
            slice_id: 2,
            chunk_id,
            offset: 500,
            length: 1000,
        };
        meta_store
            .write(file, chunk_id, slice2, 1500)
            .await
            .unwrap();

        let result = compactor.compact_light(chunk_id).await.unwrap();

        assert!(result.is_none(), "Should not remove any slices");

        let slices = meta_store.get_slices(chunk_id).await.unwrap();
        assert_eq!(slices.len(), 2, "Should still have 2 slices");
    }

    #[tokio::test]
    async fn test_compact_chunk_skips_when_no_fragmentation() {
        let (_tmp, meta_store, block_store) = setup_test_env().await;
        let compactor = Compactor::new(meta_store.clone(), block_store);

        let root = meta_store.root_ino();
        let file = meta_store
            .create_file(root, "no_frag.txt".to_string())
            .await
            .unwrap();
        let chunk_id = slayerfs::vfs::chunk_id_for(file, 0).unwrap();

        // Create non-overlapping slices (no fragmentation)
        for i in 0..3u64 {
            let slice = SliceDesc {
                slice_id: i + 1,
                chunk_id,
                offset: i * 2048, // No overlap
                length: 1024,
            };
            meta_store
                .write(file, chunk_id, slice, i * 2048 + 1024)
                .await
                .unwrap();
        }

        let result = compactor.compact_chunk(chunk_id).await.unwrap();

        assert_eq!(
            result,
            CompactResult::Skipped,
            "Should skip when no fragmentation"
        );
    }

    #[tokio::test]
    async fn test_config_thresholds() {
        use slayerfs::meta::config::CompactConfig;
        use std::time::Duration;

        let (_tmp, meta_store, block_store) = setup_test_env().await;

        // Custom config with very low thresholds
        let config = CompactConfig {
            min_slice_count: 2,       // Very low
            min_fragment_ratio: 0.01, // Very low
            async_threshold: 5,
            sync_threshold: 10,
            interval: Duration::from_secs(60),
            max_chunks_per_run: 100,
            max_concurrent_tasks: 2,
            lock_ttl: Default::default(),
            ..Default::default()
        };

        let compactor = Compactor::with_config(
            meta_store.clone(),
            block_store,
            ChunkLayout::default(),
            config,
        );

        let root = meta_store.root_ino();
        let file = meta_store
            .create_file(root, "threshold.txt".to_string())
            .await
            .unwrap();
        let chunk_id = slayerfs::vfs::chunk_id_for(file, 0).unwrap();

        // Create overlapping slices
        for i in 0..3u64 {
            let slice = SliceDesc {
                slice_id: i + 1,
                chunk_id,
                offset: i * 100,
                length: 200,
            };
            meta_store.write(file, chunk_id, slice, 500).await.unwrap();
        }

        let (should_compact, _) = compactor.should_compact(chunk_id).await.unwrap();
        assert!(should_compact, "Should trigger with low thresholds");
    }
}
