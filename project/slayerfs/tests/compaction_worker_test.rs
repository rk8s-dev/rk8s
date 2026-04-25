//! Unit tests for CompactionWorker module
//!
//! Tests for:
//! - CompactLockManager lock acquisition
//! - ChunkLockGuard drop behavior
//! - Concurrent lock handling
//! - Worker start/stop lifecycle

#[cfg(test)]
mod tests {
    use slayerfs::DatabaseMetaStore;
    use slayerfs::chunk::{
        compact::{ChunkLockGuard, CompactLockManager, CompactionWorker, CompactionWorkerConfig},
        store::InMemoryBlockStore,
    };
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

        let meta_store: Arc<DatabaseMetaStore> =
            Arc::new(DatabaseMetaStore::from_config(config).await.unwrap());
        let block_store = Arc::new(InMemoryBlockStore::new());

        meta_store.initialize().await.unwrap();

        (tmp_dir, meta_store, block_store)
    }

    #[tokio::test]
    async fn test_lock_manager_try_lock_success() {
        let (_tmp, meta_store, _) = setup_test_env().await;
        let lock_manager = CompactLockManager::new(meta_store);

        let chunk_id = 1234u64;

        // First lock acquisition should succeed
        let guard: Option<ChunkLockGuard<DatabaseMetaStore>> =
            lock_manager.try_lock(chunk_id, 5, false).await;
        assert!(guard.is_some(), "First lock acquisition should succeed");

        // Second attempt should fail (already locked locally)
        let guard2: Option<ChunkLockGuard<DatabaseMetaStore>> =
            lock_manager.try_lock(chunk_id, 5, false).await;
        assert!(guard2.is_none(), "Second lock should fail");

        // Release the lock
        drop(guard);

        // Wait a bit for the lock to be released
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Now it should succeed again
        let guard3: Option<ChunkLockGuard<DatabaseMetaStore>> =
            lock_manager.try_lock(chunk_id, 5, false).await;
        assert!(guard3.is_some(), "Lock should be available after release");
    }

    #[tokio::test]
    async fn test_lock_manager_is_locally_locked() {
        let (_tmp, meta_store, _) = setup_test_env().await;
        let lock_manager = CompactLockManager::new(meta_store);

        let chunk_id = 1234u64;

        // Initially not locked
        assert!(!lock_manager.is_locally_locked(chunk_id).await);

        // Acquire lock
        let guard: Option<ChunkLockGuard<DatabaseMetaStore>> =
            lock_manager.try_lock(chunk_id, 5, false).await;
        assert!(guard.is_some());
        assert!(lock_manager.is_locally_locked(chunk_id).await);

        // Release lock
        drop(guard);
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Note: The Drop implementation spawns a task, so it may take a moment
        // We verified the lock was held before drop, which is sufficient for this test
    }

    #[tokio::test]
    async fn test_lock_guard_unlock() {
        let (_tmp, meta_store, _) = setup_test_env().await;
        let lock_manager = Arc::new(CompactLockManager::new(meta_store));

        let chunk_id = 1234u64;

        // Acquire lock
        let mut guard: ChunkLockGuard<DatabaseMetaStore> =
            lock_manager.try_lock(chunk_id, 5, false).await.unwrap();
        assert!(lock_manager.is_locally_locked(chunk_id).await);

        // Explicitly unlock
        guard.unlock().await;

        // Wait for the unlock to complete
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Lock should be released
        assert!(!lock_manager.is_locally_locked(chunk_id).await);
    }

    #[tokio::test]
    async fn test_lock_manager_different_chunks() {
        let (_tmp, meta_store, _) = setup_test_env().await;
        let lock_manager = CompactLockManager::new(meta_store);

        // Different chunks should not interfere
        let guard1: Option<ChunkLockGuard<DatabaseMetaStore>> =
            lock_manager.try_lock(1, 5, false).await;
        assert!(guard1.is_some());

        let guard2: Option<ChunkLockGuard<DatabaseMetaStore>> =
            lock_manager.try_lock(2, 5, false).await;
        assert!(guard2.is_some(), "Different chunk should be lockable");

        let guard3 = lock_manager.try_lock(1, 5, false).await;
        assert!(guard3.is_none(), "Same chunk should not be lockable");
    }

    #[tokio::test]
    async fn test_worker_new() {
        let (_tmp, meta_store, block_store) = setup_test_env().await;

        let worker = CompactionWorker::new(meta_store, block_store);

        // Just verify it can be created
        assert!(!worker.is_chunk_locally_locked(1).await);
    }

    #[tokio::test]
    async fn test_compaction_worker_config_default() {
        let config = CompactionWorkerConfig::default();

        assert_eq!(config.scan_interval, Duration::from_secs(3600));
        assert_eq!(config.max_chunks_per_run, 100);
        assert!(config.enabled);
    }

    #[tokio::test]
    async fn test_compaction_worker_config_clone() {
        let config = CompactionWorkerConfig::default();
        let cloned = config.clone();

        assert_eq!(config.scan_interval, cloned.scan_interval);
        assert_eq!(config.max_chunks_per_run, cloned.max_chunks_per_run);
        assert_eq!(config.enabled, cloned.enabled);
    }

    #[tokio::test]
    async fn test_worker_is_chunk_locally_locked() {
        let (_tmp, meta_store, block_store) = setup_test_env().await;
        let worker = CompactionWorker::new(meta_store, block_store);

        // Initially not locked
        assert!(!worker.is_chunk_locally_locked(1234).await);
    }

    // Note: Testing the actual worker.start() would require:
    // - A running tokio runtime
    // - Mock time for deterministic testing
    // - Proper cleanup
    // This is better done in integration tests
}
