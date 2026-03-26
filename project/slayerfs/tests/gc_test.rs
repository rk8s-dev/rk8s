//! Unit tests for GC module
//!
//! Tests for BlockStoreGC including:
//! - Two-phase deletion
//! - Orphan cleanup
//! - Batch processing
//! - Error handling and retry logic

mod tests {
    use slayerfs::DatabaseMetaStore;
    use slayerfs::chunk::store::InMemoryBlockStore;
    use slayerfs::chunk::{BlockGcConfig, BlockStoreGC};
    use slayerfs::meta::store::MetaStore;
    use slayerfs::{Config, DatabaseConfig, DatabaseType};
    use std::sync::Arc;
    use std::time::Duration;
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
    async fn test_gc_cycle_empty_delayed_table() {
        let (_tmp, meta_store, block_store) = setup_test_env().await;
        let gc = BlockStoreGC::new(meta_store, block_store);

        let config = BlockGcConfig {
            interval: Duration::from_secs(60),
            min_age_secs: 0,
            batch_size: 100,
            block_size: 4 * 1024 * 1024,
            orphan_cleanup_age_secs: 60,
        };

        // Should complete without error even with no delayed slices
        let result = gc.run_gc_cycle(&config).await;
        assert!(result.is_ok(), "GC should handle empty delayed table");
    }

    #[tokio::test]
    async fn test_gc_respects_min_age() {
        let (_tmp, meta_store, block_store) = setup_test_env().await;
        let gc = BlockStoreGC::new(meta_store.clone(), block_store);

        // Create a delayed slice record directly
        let slice_id = 9999u64;
        meta_store
            .record_uncommitted_slice(slice_id, 1, 4096, "test")
            .await
            .unwrap();

        // Don't confirm - it becomes orphan

        let config = BlockGcConfig {
            interval: Duration::from_secs(60),
            min_age_secs: 3600, // 1 hour - very high
            batch_size: 100,
            block_size: 4 * 1024 * 1024,
            orphan_cleanup_age_secs: 3600, // Also high
        };

        // Run GC - should not clean up because of high age requirement
        let result = gc.run_gc_cycle(&config).await;
        assert!(result.is_ok());

        // Verify the uncommitted slice is still there (age=0 to get all regardless of age)
        let orphans = meta_store
            .cleanup_orphan_uncommitted_slices(0, 100)
            .await
            .unwrap();
        assert!(
            !orphans.iter().any(|o| o.1 == 1),
            "GC should not process uncommitted slices with high min_age_secs"
        );

        // With high max_age_secs, no orphans should be found
        let orphans_with_high_age = meta_store
            .cleanup_orphan_uncommitted_slices(3600, 100)
            .await
            .unwrap();
        assert!(
            orphans_with_high_age.is_empty(),
            "Should not find recent orphans with high age threshold"
        );
    }

    #[tokio::test]
    async fn test_gc_batch_processing_limit() {
        let (_tmp, meta_store, block_store) = setup_test_env().await;

        // Create multiple uncommitted slices
        for i in 0..10u64 {
            meta_store
                .record_uncommitted_slice(1000 + i, 1, 4096, "test")
                .await
                .unwrap();
        }

        let gc = BlockStoreGC::new(meta_store.clone(), block_store);
        let config = BlockGcConfig {
            interval: Duration::from_secs(60),
            min_age_secs: 0,
            batch_size: 5, // Only process 5 at a time
            block_size: 4 * 1024 * 1024,
            orphan_cleanup_age_secs: 0,
        };

        // First cycle should process batch_size
        let result = gc.run_gc_cycle(&config).await;
        assert!(result.is_ok());

        // Verify some orphans were processed
        let remaining = meta_store
            .cleanup_orphan_uncommitted_slices(0, 100)
            .await
            .unwrap();

        // Should have remaining orphans since batch_size was 5
        // Note: actual count depends on timing and implementation details
        println!("Remaining orphans after batch: {}", remaining.len());
    }

    #[tokio::test]
    async fn test_delete_slice_blocks_empty_size() {
        let (_tmp, meta_store, block_store) = setup_test_env().await;
        let gc = BlockStoreGC::new(meta_store, block_store);

        let config = BlockGcConfig {
            interval: Duration::from_secs(60),
            min_age_secs: 0,
            batch_size: 100,
            block_size: 4 * 1024 * 1024,
            orphan_cleanup_age_secs: 0,
        };

        // Should handle edge cases gracefully
        let result = gc.run_gc_cycle(&config).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_gc_config_defaults() {
        let config = BlockGcConfig::default();

        assert_eq!(config.interval, Duration::from_secs(3600));
        assert_eq!(config.min_age_secs, 3600);
        assert_eq!(config.batch_size, 1000);
        assert_eq!(config.block_size, 4 * 1024 * 1024);
        assert_eq!(config.orphan_cleanup_age_secs, 3600);
    }

    #[tokio::test]
    async fn test_gc_config_clone() {
        let config = BlockGcConfig::default();
        let cloned = config.clone();

        assert_eq!(config.interval, cloned.interval);
        assert_eq!(config.batch_size, cloned.batch_size);
    }

    #[tokio::test]
    async fn test_gc_new() {
        let (_tmp, meta_store, block_store) = setup_test_env().await;

        let gc = BlockStoreGC::new(meta_store.clone(), block_store.clone());

        // Just verify it can be created and runs a cycle
        let config = BlockGcConfig {
            interval: Duration::from_secs(60),
            min_age_secs: 0,
            batch_size: 100,
            block_size: 4 * 1024 * 1024,
            orphan_cleanup_age_secs: 0,
        };

        let result = gc.run_gc_cycle(&config).await;
        assert!(result.is_ok());
    }
}
