use crate::meta::MetaStore;
use crate::meta::config::Config;
use crate::meta::config::{
    CacheConfig, ClientOptions, CompactConfig, DatabaseConfig, DatabaseType,
};
use crate::meta::file_lock::{FileLockQuery, FileLockRange, FileLockType};
use crate::meta::store::{MetaError, SetAttrFlags, SetAttrRequest};
use crate::meta::stores::RedisMetaStore;
use serial_test::serial;
use std::sync::Arc;
use tokio::time;
use uuid::Uuid;

async fn cleanup_test_data() -> Result<(), MetaError> {
    let url = "redis://127.0.0.1:6379/0";
    let client = redis::Client::open(url)
        .map_err(|e| MetaError::Config(format!("Failed to create Redis client: {}", e)))?;
    let mut conn = client
        .get_multiplexed_async_connection()
        .await
        .map_err(|e| MetaError::Config(format!("Failed to connect to Redis: {}", e)))?;

    let _: () = redis::cmd("FLUSHDB")
        .query_async(&mut conn)
        .await
        .map_err(|e| MetaError::Internal(format!("Failed to flush Redis DB: {}", e)))?;

    let config = test_config();
    let _store = RedisMetaStore::from_config(config.clone())
        .await
        .map_err(|e| MetaError::Internal(format!("Failed to reinitialize root: {}", e)))?;

    Ok(())
}

fn test_config() -> Config {
    Config {
        database: DatabaseConfig {
            db_config: DatabaseType::Redis {
                url: "redis://127.0.0.1:6379/0".to_string(),
            },
        },
        cache: CacheConfig::default(),
        client: ClientOptions::default(),
        compact: CompactConfig::default(),
    }
}

/// Configuration for shared database testing (multi-session)
fn shared_db_config() -> Config {
    Config {
        database: DatabaseConfig {
            db_config: DatabaseType::Redis {
                url: "redis://127.0.0.1:6379/0".to_string(),
            },
        },
        cache: CacheConfig::default(),
        client: ClientOptions::default(),
        compact: CompactConfig::default(),
    }
}

async fn new_test_store() -> RedisMetaStore {
    if let Err(e) = cleanup_test_data().await {
        eprintln!("Failed to cleanup Redis test data: {}", e);
    }

    RedisMetaStore::from_config(test_config())
        .await
        .expect("Failed to create test database store")
}

/// Create a new test store with pre-configured session ID
async fn new_test_store_with_session(session_id: Uuid) -> RedisMetaStore {
    let store = new_test_store().await;
    store.set_sid(session_id).expect("Failed to set session ID");
    store
}

/// Helper struct to manage multiple test sessions
struct TestSessionManager {
    stores: Vec<RedisMetaStore>,
}

use std::sync::LazyLock;
use tokio::sync::Mutex;

// 静态初始化，确保只执行一次
static SHARED_DB_INIT: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

impl TestSessionManager {
    async fn new(session_count: usize) -> Self {
        // 获取锁，确保串行初始化
        let _guard = SHARED_DB_INIT.lock().await;

        use std::env;
        // Clean up existing shared test database
        let temp_dir = env::temp_dir();
        let db_path = temp_dir.join("slayerfs_shared_test.db");

        static FIRST_INIT: std::sync::Once = std::sync::Once::new();
        FIRST_INIT.call_once(|| {
            let _ = std::fs::remove_file(&db_path);
        });

        let mut stores = Vec::with_capacity(session_count);
        let mut session_ids = Vec::with_capacity(session_count);

        let config = shared_db_config();
        let first_store = RedisMetaStore::from_config(config.clone())
            .await
            .expect("Failed to create shared test database store");

        let first_session_id = Uuid::now_v7();
        first_store
            .set_sid(first_session_id)
            .expect("Failed to set session ID");

        stores.push(first_store);
        session_ids.push(first_session_id);

        for _ in 1..session_count {
            let store = RedisMetaStore::from_config(config.clone())
                .await
                .expect("Failed to create shared test database store");

            let session_id = Uuid::now_v7();
            store.set_sid(session_id).expect("Failed to set session ID");

            stores.push(store);
            session_ids.push(session_id);

            time::sleep(time::Duration::from_millis(5)).await;
        }

        Self { stores }
    }

    fn get_store(&self, index: usize) -> &RedisMetaStore {
        &self.stores[index]
    }
}

#[serial]
#[tokio::test]
#[ignore]
async fn test_symlink_roundtrip_and_unlink() {
    let store = new_test_store().await;
    let root = store.root_ino();

    let dir = store.mkdir(root, "links".to_string()).await.unwrap();
    let (ino, attr) = store
        .symlink(dir, "link.txt", "/target/path")
        .await
        .unwrap();

    assert_eq!(attr.kind, crate::meta::store::FileType::Symlink);
    assert_eq!(attr.size, "/target/path".len() as u64);
    assert_eq!(store.lookup(dir, "link.txt").await.unwrap(), Some(ino));
    assert_eq!(store.read_symlink(ino).await.unwrap(), "/target/path");

    store.unlink(dir, "link.txt").await.unwrap();
    assert_eq!(store.lookup(dir, "link.txt").await.unwrap(), None);
}

#[serial]
#[tokio::test]
#[ignore]
async fn test_hardlink_dentry_binding_cross_dir_rename_unlink() {
    let store = new_test_store().await;
    let root = store.root_ino();

    let dir_a = store.mkdir(root, "a".to_string()).await.unwrap();
    let dir_b = store.mkdir(root, "b".to_string()).await.unwrap();

    let ino = store.create_file(dir_a, "x".to_string()).await.unwrap();
    store.link(ino, dir_b, "y").await.unwrap();

    let names = store.get_names(ino).await.unwrap();
    assert!(names.contains(&(Some(dir_a), "x".to_string())));
    assert!(names.contains(&(Some(dir_b), "y".to_string())));

    assert_eq!(store.lookup(dir_a, "x").await.unwrap(), Some(ino));
    assert_eq!(store.lookup(dir_b, "y").await.unwrap(), Some(ino));

    store
        .rename(dir_b, "y", dir_b, "z".to_string())
        .await
        .unwrap();

    let names = store.get_names(ino).await.unwrap();
    assert!(names.contains(&(Some(dir_a), "x".to_string())));
    assert!(names.contains(&(Some(dir_b), "z".to_string())));
    assert!(!names.contains(&(Some(dir_b), "y".to_string())));

    assert_eq!(store.lookup(dir_b, "y").await.unwrap(), None);
    assert_eq!(store.lookup(dir_b, "z").await.unwrap(), Some(ino));
    assert_eq!(store.lookup(dir_a, "x").await.unwrap(), Some(ino));

    store.unlink(dir_a, "x").await.unwrap();

    let names = store.get_names(ino).await.unwrap();
    assert_eq!(names, vec![(Some(dir_b), "z".to_string())]);
    assert_eq!(store.lookup(dir_b, "z").await.unwrap(), Some(ino));
}

#[serial]
#[tokio::test]
#[ignore]
async fn test_hardlink_dentry_binding_cross_dir_move_rename() {
    let store = new_test_store().await;
    let root = store.root_ino();

    let dir_a = store.mkdir(root, "a".to_string()).await.unwrap();
    let dir_b = store.mkdir(root, "b".to_string()).await.unwrap();
    let dir_c = store.mkdir(root, "c".to_string()).await.unwrap();

    let ino = store.create_file(dir_a, "x".to_string()).await.unwrap();
    store.link(ino, dir_b, "y").await.unwrap();

    assert_eq!(store.lookup(dir_a, "x").await.unwrap(), Some(ino));
    assert_eq!(store.lookup(dir_b, "y").await.unwrap(), Some(ino));

    store
        .rename(dir_b, "y", dir_c, "z".to_string())
        .await
        .unwrap();

    let names = store.get_names(ino).await.unwrap();
    assert!(names.contains(&(Some(dir_a), "x".to_string())));
    assert!(names.contains(&(Some(dir_c), "z".to_string())));
    assert!(!names.contains(&(Some(dir_b), "y".to_string())));

    assert_eq!(store.lookup(dir_b, "y").await.unwrap(), None);
    assert_eq!(store.lookup(dir_c, "z").await.unwrap(), Some(ino));
    assert_eq!(store.lookup(dir_a, "x").await.unwrap(), Some(ino));
}

#[serial]
#[tokio::test]
#[ignore]
async fn test_basic_read_lock() {
    let store = new_test_store().await;
    let session_id = Uuid::now_v7();
    let owner: i64 = 1001;

    // Set session
    store.set_sid(session_id).unwrap();

    // Create a file first
    let parent = store.root_ino();
    let file_ino = store
        .create_file(parent, "test_basic_read_lock_file.txt".to_string())
        .await
        .unwrap();

    // Acquire read lock
    store
        .set_plock(
            file_ino,
            owner,
            false,
            FileLockType::Read,
            FileLockRange { start: 0, end: 100 },
            1234,
        )
        .await
        .unwrap();

    // Verify lock exists
    let query = FileLockQuery {
        owner,
        lock_type: FileLockType::Read,
        range: FileLockRange { start: 0, end: 100 },
    };

    let lock_info = store.get_plock(file_ino, &query).await.unwrap();
    assert_eq!(lock_info.lock_type, FileLockType::UnLock);
}

#[serial]
#[tokio::test]
#[ignore]
async fn test_multiple_read_locks() {
    // Create session manager with 2 sessions
    let session_mgr = TestSessionManager::new(2).await;

    let owner1: i64 = 1001;
    let owner2: i64 = 1002;

    // Create a file first using the first session
    let store1 = session_mgr.get_store(0);
    let parent = store1.root_ino();
    let file_ino = store1
        .create_file(
            parent,
            format!("test_multiple_read_locks_{}.txt", Uuid::now_v7()),
        )
        .await
        .unwrap();

    // First session acquires read lock
    store1
        .set_plock(
            file_ino,
            owner1,
            false,
            FileLockType::Read,
            FileLockRange { start: 0, end: 100 },
            1234,
        )
        .await
        .unwrap();

    // Second session should be able to acquire read lock on same range
    let store2 = session_mgr.get_store(1);
    store2
        .set_plock(
            file_ino,
            owner2,
            false,
            FileLockType::Read,
            FileLockRange { start: 0, end: 100 },
            5678,
        )
        .await
        .unwrap();

    // Verify both locks exist by querying each session
    let query1 = FileLockQuery {
        owner: owner1,
        lock_type: FileLockType::Write,
        range: FileLockRange { start: 0, end: 100 },
    };

    let query2 = FileLockQuery {
        owner: owner2,
        lock_type: FileLockType::Read,
        range: FileLockRange { start: 0, end: 100 },
    };

    let lock_info1 = store1.get_plock(file_ino, &query1).await.unwrap();
    assert_eq!(lock_info1.lock_type, FileLockType::Read);
    assert_eq!(lock_info1.range.start, 0);
    assert_eq!(lock_info1.range.end, 100);
    assert_eq!(lock_info1.pid, 1234);

    let lock_info2 = store2.get_plock(file_ino, &query2).await.unwrap();
    assert_eq!(lock_info2.lock_type, FileLockType::UnLock);
}

#[serial]
#[tokio::test]
#[ignore]
async fn test_write_lock_conflict() {
    // Create session manager with 2 sessions
    let session_mgr = TestSessionManager::new(2).await;

    let owner1: u64 = 1001;
    let owner2: u64 = 1002;

    // Create a file first using the first session
    let store1 = session_mgr.get_store(0);
    let parent = store1.root_ino();
    let file_ino = store1
        .create_file(parent, "test_write_lock_conflict_file.txt".to_string())
        .await
        .unwrap();

    // First session acquires read lock
    store1
        .set_plock(
            file_ino,
            owner1 as i64,
            false,
            FileLockType::Read,
            FileLockRange { start: 0, end: 100 },
            1234,
        )
        .await
        .unwrap();

    // Second session should not be able to acquire write lock on overlapping range
    let store2 = session_mgr.get_store(1);
    let result = store2
        .set_plock(
            file_ino,
            owner2 as i64,
            false, // non-blocking
            FileLockType::Write,
            FileLockRange {
                start: 50,
                end: 150,
            }, // Overlapping range
            5678,
        )
        .await;

    assert!(result.is_err());
    match result.unwrap_err() {
        MetaError::LockConflict {
            inode: err_inode,
            owner: err_owner,
            range: err_range,
        } => {
            assert_eq!(err_inode, file_ino);
            assert_eq!(err_owner, owner2 as i64);
            assert_eq!(err_range.start, 50);
            assert_eq!(err_range.end, 150);
        }
        _ => panic!("Expected LockConflict error"),
    }
}

#[serial]
#[tokio::test]
#[ignore]
async fn test_lock_release() {
    let session_id = Uuid::now_v7();
    let owner = 1001;

    // Create a store with pre-configured session
    let store = new_test_store_with_session(session_id).await;

    // Create a file first
    let parent = store.root_ino();
    let file_ino = store
        .create_file(parent, "test_lock_release_file.txt".to_string())
        .await
        .unwrap();

    // Acquire lock
    store
        .set_plock(
            file_ino,
            owner,
            false,
            FileLockType::Write,
            FileLockRange { start: 0, end: 100 },
            1234,
        )
        .await
        .unwrap();

    // Verify lock exists
    let query = FileLockQuery {
        owner,
        lock_type: FileLockType::Write,
        range: FileLockRange { start: 0, end: 100 },
    };

    let lock_info = store.get_plock(file_ino, &query).await.unwrap();
    assert_eq!(lock_info.lock_type, FileLockType::Write);

    // Release lock
    store
        .set_plock(
            file_ino,
            owner,
            false,
            FileLockType::UnLock,
            FileLockRange { start: 0, end: 100 },
            1234,
        )
        .await
        .unwrap();

    // Verify lock is released
    let lock_info = store.get_plock(file_ino, &query).await.unwrap();
    assert_eq!(lock_info.lock_type, FileLockType::UnLock);
}

#[serial]
#[tokio::test]
#[ignore]
async fn test_non_overlapping_locks() {
    // Create session manager with 2 sessions
    let session_mgr = TestSessionManager::new(2).await;

    let owner1: i64 = 1001;
    let owner2: i64 = 1002;

    // Create a file first using the first session
    let store1 = session_mgr.get_store(0);
    let parent = store1.root_ino();
    let file_ino = store1
        .create_file(parent, "test_none_overlapping_locks_file.txt".to_string())
        .await
        .unwrap();

    // First session acquires lock on range 0-100
    store1
        .set_plock(
            file_ino,
            owner1,
            false,
            FileLockType::Write,
            FileLockRange { start: 0, end: 100 },
            1234,
        )
        .await
        .unwrap();

    // Second session should be able to acquire lock on non-overlapping range 200-300
    let store2 = session_mgr.get_store(1);
    store2
        .set_plock(
            file_ino,
            owner2,
            false,
            FileLockType::Write,
            FileLockRange {
                start: 200,
                end: 300,
            },
            5678,
        )
        .await
        .unwrap();

    // Verify both locks exist
    let query1 = FileLockQuery {
        owner: owner1,
        lock_type: FileLockType::Write,
        range: FileLockRange { start: 0, end: 100 },
    };

    let query2 = FileLockQuery {
        owner: owner2,
        lock_type: FileLockType::Write,
        range: FileLockRange {
            start: 200,
            end: 300,
        },
    };

    let lock_info1 = store1.get_plock(file_ino, &query1).await.unwrap();
    assert_eq!(lock_info1.lock_type, FileLockType::Write);
    assert_eq!(lock_info1.range.start, 0);
    assert_eq!(lock_info1.range.end, 100);
    assert_eq!(lock_info1.pid, 1234);

    let lock_info2 = store2.get_plock(file_ino, &query2).await.unwrap();
    assert_eq!(lock_info2.lock_type, FileLockType::Write);
    assert_eq!(lock_info2.range.start, 200);
    assert_eq!(lock_info2.range.end, 300);
    assert_eq!(lock_info2.pid, 5678);
}

#[serial]
#[tokio::test]
#[ignore]
async fn test_concurrent_read_write_locks() {
    // Test multiple sessions acquiring different types of locks
    let session_mgr = TestSessionManager::new(3).await;

    // Create a file
    let store0 = session_mgr.get_store(0);
    let parent = store0.root_ino();
    let file_ino = store0
        .create_file(parent, "test_concurrent_read_write_locks.txt".to_string())
        .await
        .unwrap();

    let owner1: i64 = 1001;
    let owner2: i64 = 1002;
    let owner3: i64 = 1003;

    // Session 1: Acquire write lock on range 0-100
    {
        let store1 = session_mgr.get_store(0);
        store1
            .set_plock(
                file_ino,
                owner1,
                false,
                FileLockType::Write,
                FileLockRange { start: 0, end: 100 },
                1111,
            )
            .await
            .expect("Failed to acquire write lock");
    }

    // Session 2: Acquire read lock on range 200-300 (should succeed)
    {
        let store2 = session_mgr.get_store(1);
        store2
            .set_plock(
                file_ino,
                owner2,
                false,
                FileLockType::Read,
                FileLockRange {
                    start: 200,
                    end: 300,
                },
                2222,
            )
            .await
            .expect("Failed to acquire read lock");
    }

    // Session 3: Try to acquire write lock on overlapping range 50-150 (should fail)
    {
        let store3 = session_mgr.get_store(2);
        let result = store3
            .set_plock(
                file_ino,
                owner3,
                false,
                FileLockType::Write,
                FileLockRange {
                    start: 50,
                    end: 150,
                },
                3333,
            )
            .await;

        // Verify it fails with LockConflict
        assert!(result.is_err());
        match result.unwrap_err() {
            MetaError::LockConflict { .. } => {}
            _ => panic!("Expected LockConflict error"),
        }
    }

    // Verify successful locks exist
    let query1 = FileLockQuery {
        owner: owner1,
        lock_type: FileLockType::Write,
        range: FileLockRange { start: 0, end: 100 },
    };

    let query2 = FileLockQuery {
        owner: owner2,
        lock_type: FileLockType::Read,
        range: FileLockRange {
            start: 200,
            end: 300,
        },
    };

    // Check locks from different sessions
    {
        let store1 = session_mgr.get_store(0);
        let lock_info1 = store1.get_plock(file_ino, &query1).await.unwrap();
        assert_eq!(lock_info1.lock_type, FileLockType::Write);
    }

    {
        let store2 = session_mgr.get_store(1);
        let lock_info2 = store2.get_plock(file_ino, &query2).await.unwrap();
        assert_eq!(lock_info2.lock_type, FileLockType::UnLock);
    }
}

#[serial]
#[tokio::test]
#[ignore]
async fn test_cross_session_lock_visibility() {
    // Test that locks set by one session are visible to another session
    let session_mgr = TestSessionManager::new(2).await;

    let owner1: u64 = 1001;

    // Create a file
    let store1 = session_mgr.get_store(0);
    let parent = store1.root_ino();
    let file_ino = store1
        .create_file(parent, "test_cross_session_lock_visibility.txt".to_string())
        .await
        .unwrap();

    // Session 1 acquires a write lock
    store1
        .set_plock(
            file_ino,
            owner1 as i64,
            false,
            FileLockType::Write,
            FileLockRange {
                start: 0,
                end: 1000,
            },
            4444,
        )
        .await
        .unwrap();

    // Session 2 should be able to see the lock (and respect it)
    let store2 = session_mgr.get_store(1);
    let conflict_result = store2
        .set_plock(
            file_ino,
            2002, // different owner
            false,
            FileLockType::Write,
            FileLockRange {
                start: 500,
                end: 600,
            }, // overlapping range
            5555,
        )
        .await;

    // Should fail due to lock conflict
    assert!(conflict_result.is_err());
    match conflict_result.unwrap_err() {
        MetaError::LockConflict { .. } => {}
        _ => panic!("Expected LockConflict error"),
    }

    // Session 1 releases the lock
    store1
        .set_plock(
            file_ino,
            owner1 as i64,
            false,
            FileLockType::UnLock,
            FileLockRange {
                start: 0,
                end: 1000,
            },
            4444,
        )
        .await
        .unwrap();

    // Now Session 2 should be able to acquire the lock
    store2
        .set_plock(
            file_ino,
            2002,
            false,
            FileLockType::Write,
            FileLockRange {
                start: 500,
                end: 600,
            },
            5555,
        )
        .await
        .unwrap();

    // Verify the lock exists
    let query = FileLockQuery {
        owner: 2002,
        lock_type: FileLockType::Write,
        range: FileLockRange {
            start: 500,
            end: 600,
        },
    };

    let lock_info = store2.get_plock(file_ino, &query).await.unwrap();
    assert_eq!(lock_info.lock_type, FileLockType::Write);
    assert_eq!(lock_info.pid, 5555);
}

#[serial]
#[tokio::test]
#[ignore]
async fn test_extend_file_size_lua_concurrent() {
    use crate::meta::MetaStore;

    let store = new_test_store().await;
    let root = store.root_ino();
    let ino = store
        .create_file(root, "test.txt".to_string())
        .await
        .unwrap();

    let store1 = std::sync::Arc::new(store);
    let store2 = store1.clone();
    let store3 = store1.clone();
    let store4 = store1.clone();

    let h1 = tokio::spawn(async move { store2.extend_file_size(ino, 1000).await });
    let h2 = tokio::spawn(async move { store3.extend_file_size(ino, 2000).await });
    let h3 = tokio::spawn(async move { store4.extend_file_size(ino, 1500).await });

    h1.await.unwrap().unwrap();
    h2.await.unwrap().unwrap();
    h3.await.unwrap().unwrap();

    let attr = store1.stat(ino).await.unwrap().unwrap();
    assert_eq!(attr.size, 2000);
}

#[serial]
#[tokio::test]
#[ignore]
async fn test_extend_file_size_lua_idempotent() {
    use crate::meta::MetaStore;

    let store = new_test_store().await;
    let root = store.root_ino();
    let ino = store
        .create_file(root, "test.txt".to_string())
        .await
        .unwrap();

    store.extend_file_size(ino, 1000).await.unwrap();
    let attr1 = store.stat(ino).await.unwrap().unwrap();
    assert_eq!(attr1.size, 1000);

    store.extend_file_size(ino, 500).await.unwrap();
    let attr2 = store.stat(ino).await.unwrap().unwrap();
    assert_eq!(attr2.size, 1000);

    store.extend_file_size(ino, 1000).await.unwrap();
    let attr3 = store.stat(ino).await.unwrap().unwrap();
    assert_eq!(attr3.size, 1000);
}

#[serial]
#[tokio::test]
#[ignore]
async fn test_extend_file_size_lua_missing_node() {
    use crate::meta::MetaStore;

    let store = new_test_store().await;
    let result = store.extend_file_size(99999, 1000).await;

    assert!(result.is_err());
    match result.unwrap_err() {
        MetaError::NotFound(ino) => assert_eq!(ino, 99999),
        other => panic!("expected NotFound error, got {:?}", other),
    }
}

#[serial]
#[tokio::test]
#[ignore]
async fn test_link_unlink_lua_atomicity() {
    use crate::meta::MetaStore;

    let store = new_test_store().await;
    let root = store.root_ino();

    let dir_a = store.mkdir(root, "a".to_string()).await.unwrap();
    let dir_b = store.mkdir(root, "b".to_string()).await.unwrap();

    let ino = store.create_file(dir_a, "x".to_string()).await.unwrap();

    let attr1 = store.link(ino, dir_b, "y").await.unwrap();
    assert_eq!(attr1.nlink, 2);

    assert_eq!(store.lookup(dir_a, "x").await.unwrap(), Some(ino));
    assert_eq!(store.lookup(dir_b, "y").await.unwrap(), Some(ino));

    let result = store.link(ino, dir_b, "y").await;
    assert!(result.is_err());
    match result.unwrap_err() {
        MetaError::AlreadyExists { parent, name } => {
            assert_eq!(parent, dir_b);
            assert_eq!(name, "y");
        }
        other => panic!("expected AlreadyExists error, got {:?}", other),
    }

    store.unlink(dir_a, "x").await.unwrap();
    assert_eq!(store.lookup(dir_a, "x").await.unwrap(), None);
    assert_eq!(store.lookup(dir_b, "y").await.unwrap(), Some(ino));

    let attr2 = store.stat(ino).await.unwrap().unwrap();
    assert_eq!(attr2.nlink, 1);

    store.unlink(dir_b, "y").await.unwrap();
    assert_eq!(store.lookup(dir_b, "y").await.unwrap(), None);

    let deleted = store.get_deleted_files().await.unwrap();
    assert!(deleted.contains(&ino));
}

#[serial]
#[tokio::test]
#[ignore]
async fn test_rmdir_lua_concurrent() {
    let store = Arc::new(new_test_store().await);
    let root = store.root_ino();

    let _test_dir = store.mkdir(root, "testdir".to_string()).await.unwrap();

    let store1 = store.clone();
    let store2 = store.clone();
    let store3 = store.clone();
    let store4 = store.clone();

    let h1 = tokio::spawn(async move { store1.rmdir(root, "testdir").await });
    let h2 = tokio::spawn(async move { store2.rmdir(root, "testdir").await });
    let h3 = tokio::spawn(async move { store3.rmdir(root, "testdir").await });
    let h4 = tokio::spawn(async move { store4.rmdir(root, "testdir").await });

    let r1 = h1.await.unwrap();
    let r2 = h2.await.unwrap();
    let r3 = h3.await.unwrap();
    let r4 = h4.await.unwrap();

    let results = [r1, r2, r3, r4];
    let success_count = results.iter().filter(|r| r.is_ok()).count();
    assert_eq!(
        success_count, 1,
        "Exactly one rmdir should succeed, got {} successes",
        success_count
    );

    let not_found_count = results
        .iter()
        .filter(|r| matches!(r, Err(MetaError::NotFound(ino)) if ino == &root))
        .count();
    assert_eq!(
        not_found_count, 3,
        "Three rmdir should return NotFound(parent), got {}",
        not_found_count
    );

    assert_eq!(store.lookup(root, "testdir").await.unwrap(), None);
}

#[serial]
#[tokio::test]
#[ignore]
async fn test_rmdir_lua_not_empty() {
    let store = new_test_store().await;
    let root = store.root_ino();

    let parent_dir = store.mkdir(root, "parent".to_string()).await.unwrap();
    let _child_dir = store.mkdir(parent_dir, "child".to_string()).await.unwrap();

    let result = store.rmdir(root, "parent").await;
    assert!(result.is_err());
    match result.unwrap_err() {
        MetaError::DirectoryNotEmpty(ino) => assert_eq!(ino, parent_dir),
        other => panic!("expected DirectoryNotEmpty error, got {:?}", other),
    }
}

#[serial]
#[tokio::test]
#[ignore]
async fn test_rmdir_lua_not_found() {
    let store = new_test_store().await;
    let root = store.root_ino();

    let result = store.rmdir(root, "nonexistent").await;
    assert!(result.is_err());
    match result.unwrap_err() {
        MetaError::NotFound(ino) => assert_eq!(ino, root),
        other => panic!("expected NotFound(parent) error, got {:?}", other),
    }
}

#[serial]
#[tokio::test]
#[ignore]
async fn test_rmdir_lua_not_directory() {
    let store = new_test_store().await;
    let root = store.root_ino();

    let file_ino = store
        .create_file(root, "file.txt".to_string())
        .await
        .unwrap();

    let result = store.rmdir(root, "file.txt").await;
    assert!(result.is_err());
    match result.unwrap_err() {
        MetaError::NotDirectory(ino) => assert_eq!(ino, file_ino),
        other => panic!("expected NotDirectory error, got {:?}", other),
    }
}

#[serial]
#[tokio::test]
#[ignore]
async fn test_create_entry_lua_concurrent() {
    let store = Arc::new(new_test_store().await);
    let root = store.root_ino();

    let store1 = store.clone();
    let store2 = store.clone();
    let store3 = store.clone();
    let store4 = store.clone();

    let h1 = tokio::spawn(async move { store1.mkdir(root, "newdir".to_string()).await });
    let h2 = tokio::spawn(async move { store2.mkdir(root, "newdir".to_string()).await });
    let h3 = tokio::spawn(async move { store3.mkdir(root, "newdir".to_string()).await });
    let h4 = tokio::spawn(async move { store4.mkdir(root, "newdir".to_string()).await });

    let r1 = h1.await.unwrap();
    let r2 = h2.await.unwrap();
    let r3 = h3.await.unwrap();
    let r4 = h4.await.unwrap();

    let results = [r1, r2, r3, r4];
    let success_count = results.iter().filter(|r| r.is_ok()).count();
    assert_eq!(
        success_count, 1,
        "Exactly one mkdir should succeed, got {} successes",
        success_count
    );

    let already_exists_count = results
            .iter()
            .filter(|r| matches!(r, Err(MetaError::AlreadyExists { parent, name }) if parent == &root && name == "newdir"))
            .count();
    assert_eq!(
        already_exists_count, 3,
        "Three mkdir should return AlreadyExists, got {}",
        already_exists_count
    );

    let ino = store.lookup(root, "newdir").await.unwrap();
    assert!(ino.is_some());
}

#[serial]
#[tokio::test]
#[ignore]
async fn test_create_entry_lua_already_exists() {
    let store = new_test_store().await;
    let root = store.root_ino();

    store.mkdir(root, "existing".to_string()).await.unwrap();

    let result = store.mkdir(root, "existing".to_string()).await;
    assert!(result.is_err());
    match result.unwrap_err() {
        MetaError::AlreadyExists { parent, name } => {
            assert_eq!(parent, root);
            assert_eq!(name, "existing");
        }
        other => panic!("expected AlreadyExists error, got {:?}", other),
    }
}

#[serial]
#[tokio::test]
#[ignore]
async fn test_create_entry_lua_parent_not_found() {
    let store = new_test_store().await;

    let result = store.mkdir(999999, "newdir".to_string()).await;
    assert!(result.is_err());
    match result.unwrap_err() {
        MetaError::ParentNotFound(ino) => assert_eq!(ino, 999999),
        other => panic!("expected ParentNotFound error, got {:?}", other),
    }
}

#[serial]
#[tokio::test]
#[ignore]
async fn test_create_entry_lua_parent_not_directory() {
    let store = new_test_store().await;
    let root = store.root_ino();

    let file_ino = store
        .create_file(root, "file.txt".to_string())
        .await
        .unwrap();

    let result = store.mkdir(file_ino, "newdir".to_string()).await;
    assert!(result.is_err());
    match result.unwrap_err() {
        MetaError::NotDirectory(ino) => assert_eq!(ino, file_ino),
        other => panic!("expected NotDirectory error, got {:?}", other),
    }
}

#[serial]
#[tokio::test]
#[ignore]
async fn test_rename_lua_concurrent() {
    let store = Arc::new(new_test_store().await);
    let root = store.root_ino();

    let file_ino = store
        .create_file(root, "file.txt".to_string())
        .await
        .unwrap();
    store.mkdir(root, "dir1".to_string()).await.unwrap();
    store.mkdir(root, "dir2".to_string()).await.unwrap();
    store.mkdir(root, "dir3".to_string()).await.unwrap();
    store.mkdir(root, "dir4".to_string()).await.unwrap();

    let store1 = store.clone();
    let store2 = store.clone();
    let store3 = store.clone();
    let store4 = store.clone();

    let h1 = tokio::spawn(async move {
        store1
            .rename(root, "file.txt", root, "moved1.txt".to_string())
            .await
    });
    let h2 = tokio::spawn(async move {
        store2
            .rename(root, "file.txt", root, "moved2.txt".to_string())
            .await
    });
    let h3 = tokio::spawn(async move {
        store3
            .rename(root, "file.txt", root, "moved3.txt".to_string())
            .await
    });
    let h4 = tokio::spawn(async move {
        store4
            .rename(root, "file.txt", root, "moved4.txt".to_string())
            .await
    });

    let r1 = h1.await.unwrap();
    let r2 = h2.await.unwrap();
    let r3 = h3.await.unwrap();
    let r4 = h4.await.unwrap();

    let success_count = [&r1, &r2, &r3, &r4].iter().filter(|r| r.is_ok()).count();
    assert_eq!(success_count, 1, "exactly one rename should succeed");

    let not_found_count = [&r1, &r2, &r3, &r4]
        .iter()
        .filter(|r| matches!(r, Err(MetaError::NotFound(ino)) if *ino == root))
        .count();
    assert_eq!(
        not_found_count, 3,
        "three renames should return NotFound(parent)"
    );

    let final_node = store.get_node(file_ino).await.unwrap().unwrap();
    assert!(
        final_node.name.starts_with("moved") && final_node.name.ends_with(".txt"),
        "file should be renamed to one of the target names"
    );
}

#[serial]
#[tokio::test]
#[ignore]
async fn test_rename_lua_source_not_found() {
    let store = new_test_store().await;
    let root = store.root_ino();

    let result = store
        .rename(root, "nonexistent.txt", root, "moved.txt".to_string())
        .await;
    assert!(result.is_err());
    match result.unwrap_err() {
        MetaError::NotFound(ino) => assert_eq!(ino, root),
        other => panic!("expected NotFound(parent) error, got {:?}", other),
    }
}

#[serial]
#[tokio::test]
#[ignore]
async fn test_rename_lua_target_exists() {
    let store = new_test_store().await;
    let root = store.root_ino();

    store
        .create_file(root, "file1.txt".to_string())
        .await
        .unwrap();
    store
        .create_file(root, "file2.txt".to_string())
        .await
        .unwrap();

    let result = store
        .rename(root, "file1.txt", root, "file2.txt".to_string())
        .await;
    assert!(result.is_err());
    match result.unwrap_err() {
        MetaError::AlreadyExists { parent, name } => {
            assert_eq!(parent, root);
            assert_eq!(name, "file2.txt");
        }
        other => panic!("expected AlreadyExists error, got {:?}", other),
    }
}

#[serial]
#[tokio::test]
#[ignore]
async fn test_rename_lua_overwrite_file() {
    let store = new_test_store().await;
    let root = store.root_ino();

    let src_ino = store
        .create_file(root, "src.txt".to_string())
        .await
        .unwrap();
    let dst_ino = store
        .create_file(root, "dst.txt".to_string())
        .await
        .unwrap();

    store
        .rename(root, "src.txt", root, "dst.txt".to_string())
        .await
        .unwrap();

    assert_eq!(store.lookup(root, "src.txt").await.unwrap(), None);
    assert_eq!(store.lookup(root, "dst.txt").await.unwrap(), Some(src_ino));

    let overwritten = store.get_node(dst_ino).await.unwrap().unwrap();
    assert!(
        overwritten.deleted,
        "overwritten inode should be tombstoned"
    );
    assert_eq!(
        overwritten.attr.nlink, 0,
        "overwritten inode should have nlink=0"
    );
    assert!(
        store.stat(dst_ino).await.unwrap().is_some(),
        "tombstoned inode should remain until GC"
    );

    let deleted = store.get_deleted_files().await.unwrap();
    assert!(
        deleted.contains(&dst_ino),
        "overwritten inode should be queued for cleanup"
    );
}

#[serial]
#[tokio::test]
#[ignore]
async fn test_rename_lua_directory_replaces_empty_directory() {
    let store = new_test_store().await;
    let root = store.root_ino();

    let src_ino = store.mkdir(root, "src".to_string()).await.unwrap();
    let dst_ino = store.mkdir(root, "dst".to_string()).await.unwrap();

    store
        .rename(root, "src", root, "dst".to_string())
        .await
        .unwrap();

    assert_eq!(store.lookup(root, "src").await.unwrap(), None);
    assert_eq!(store.lookup(root, "dst").await.unwrap(), Some(src_ino));
    assert!(store.get_node(dst_ino).await.unwrap().is_none());
    assert!(store.get_node(src_ino).await.unwrap().is_some());
}

#[serial]
#[tokio::test]
#[ignore]
async fn test_rename_lua_directory_to_missing_target_updates_parent_name() {
    let store = new_test_store().await;
    let root = store.root_ino();

    let src_ino = store.mkdir(root, "src".to_string()).await.unwrap();

    store
        .rename(root, "src", root, "dst".to_string())
        .await
        .unwrap();

    assert_eq!(store.lookup(root, "src").await.unwrap(), None);
    assert_eq!(store.lookup(root, "dst").await.unwrap(), Some(src_ino));

    let renamed = store.get_node(src_ino).await.unwrap().unwrap();
    assert_eq!(renamed.parent, root);
    assert_eq!(renamed.name, "dst");
}

#[serial]
#[tokio::test]
#[ignore]
async fn test_rename_lua_cross_dir_directory_preserves_parent_nlink_after_cleanup() {
    let store = new_test_store().await;
    let root = store.root_ino();

    let dir_x = store.mkdir(root, "x".to_string()).await.unwrap();
    let dir_y = store.mkdir(root, "y".to_string()).await.unwrap();
    store.mkdir(dir_x, "src".to_string()).await.unwrap();
    store.mkdir(dir_y, "dst".to_string()).await.unwrap();

    // Mirror the VFS overwrite flow: remove the empty destination first, then rename.
    store.rmdir(dir_y, "dst").await.unwrap();
    store
        .rename(dir_x, "src", dir_y, "dst".to_string())
        .await
        .unwrap();

    let y_attr = store.stat(dir_y).await.unwrap().unwrap();
    assert_eq!(
        y_attr.nlink, 3,
        "moved subdir should increment new parent nlink"
    );

    store.rmdir(dir_y, "dst").await.unwrap();

    let y_attr = store.stat(dir_y).await.unwrap().unwrap();
    assert_eq!(y_attr.nlink, 2, "cleanup should restore parent nlink");
    assert_eq!(store.lookup(root, "y").await.unwrap(), Some(dir_y));
    assert_eq!(
        store.get_paths(dir_y).await.unwrap(),
        vec!["/y".to_string()]
    );
}

#[serial]
#[tokio::test]
#[ignore]
async fn test_rename_lua_directory_rejects_non_empty_directory_target() {
    let store = new_test_store().await;
    let root = store.root_ino();

    let src_ino = store.mkdir(root, "src".to_string()).await.unwrap();
    let dst_ino = store.mkdir(root, "dst".to_string()).await.unwrap();
    store
        .create_file(dst_ino, "child.txt".to_string())
        .await
        .unwrap();

    let result = store.rename(root, "src", root, "dst".to_string()).await;
    match result.unwrap_err() {
        MetaError::DirectoryNotEmpty(ino) => assert_eq!(ino, dst_ino),
        other => panic!("expected DirectoryNotEmpty error, got {:?}", other),
    }

    assert_eq!(store.lookup(root, "src").await.unwrap(), Some(src_ino));
    assert_eq!(store.lookup(root, "dst").await.unwrap(), Some(dst_ino));
}

#[serial]
#[tokio::test]
#[ignore]
async fn test_rename_lua_directory_rejects_file_target() {
    let store = new_test_store().await;
    let root = store.root_ino();

    let src_ino = store.mkdir(root, "src".to_string()).await.unwrap();
    let dst_ino = store
        .create_file(root, "dst.txt".to_string())
        .await
        .unwrap();

    let result = store.rename(root, "src", root, "dst.txt".to_string()).await;
    match result.unwrap_err() {
        MetaError::Io(err) => assert_eq!(err.kind(), std::io::ErrorKind::NotADirectory),
        other => panic!("expected NotADirectory IO error, got {:?}", other),
    }

    assert_eq!(store.lookup(root, "src").await.unwrap(), Some(src_ino));
    assert_eq!(store.lookup(root, "dst.txt").await.unwrap(), Some(dst_ino));
}

#[serial]
#[tokio::test]
#[ignore]
async fn test_rename_lua_file_rejects_directory_target() {
    let store = new_test_store().await;
    let root = store.root_ino();

    let src_ino = store
        .create_file(root, "src.txt".to_string())
        .await
        .unwrap();
    let dst_ino = store.mkdir(root, "dst".to_string()).await.unwrap();

    let result = store.rename(root, "src.txt", root, "dst".to_string()).await;
    match result.unwrap_err() {
        MetaError::Io(err) => assert_eq!(err.kind(), std::io::ErrorKind::IsADirectory),
        other => panic!("expected IsADirectory IO error, got {:?}", other),
    }

    assert_eq!(store.lookup(root, "src.txt").await.unwrap(), Some(src_ino));
    assert_eq!(store.lookup(root, "dst").await.unwrap(), Some(dst_ino));
}

#[serial]
#[tokio::test]
#[ignore]
async fn test_rename_lua_same_name() {
    let store = new_test_store().await;
    let root = store.root_ino();

    let file_ino = store
        .create_file(root, "file.txt".to_string())
        .await
        .unwrap();
    let node_before = store.get_node(file_ino).await.unwrap().unwrap();

    let result = store
        .rename(root, "file.txt", root, "file.txt".to_string())
        .await;
    assert!(result.is_ok(), "self-rename should be no-op");

    let node_after = store.get_node(file_ino).await.unwrap().unwrap();
    assert_eq!(
        node_before.attr.mtime, node_after.attr.mtime,
        "mtime should not change"
    );
    assert_eq!(
        node_before.attr.ctime, node_after.attr.ctime,
        "ctime should not change"
    );
}

#[serial]
#[tokio::test]
#[ignore]
async fn test_rename_lua_hardlink() {
    let store = new_test_store().await;
    let root = store.root_ino();

    let file_ino = store
        .create_file(root, "file.txt".to_string())
        .await
        .unwrap();
    store.link(file_ino, root, "link.txt").await.unwrap();

    let node_before = store.get_node(file_ino).await.unwrap().unwrap();
    assert_eq!(node_before.attr.nlink, 2, "file should have nlink=2");
    assert_eq!(
        node_before.parent, 0,
        "hardlinked file should have parent=0"
    );
    assert_eq!(node_before.name, "", "hardlinked file should have name=''");

    let link_parents_before = store.load_link_parents(file_ino).await.unwrap();
    assert_eq!(link_parents_before.len(), 2);
    assert!(link_parents_before.contains(&(root, "file.txt".to_string())));
    assert!(link_parents_before.contains(&(root, "link.txt".to_string())));

    let result = store
        .rename(root, "file.txt", root, "renamed.txt".to_string())
        .await;
    assert!(result.is_ok());

    let node_after = store.get_node(file_ino).await.unwrap().unwrap();
    assert_eq!(node_after.attr.nlink, 2, "nlink should remain 2");
    assert_eq!(node_after.parent, 0, "parent should remain 0");
    assert_eq!(node_after.name, "", "name should remain ''");

    let link_parents_after = store.load_link_parents(file_ino).await.unwrap();
    assert_eq!(link_parents_after.len(), 2);
    assert!(link_parents_after.contains(&(root, "renamed.txt".to_string())));
    assert!(link_parents_after.contains(&(root, "link.txt".to_string())));
    assert!(!link_parents_after.contains(&(root, "file.txt".to_string())));
}

#[serial]
#[tokio::test]
#[ignore]
async fn test_rename_exchange_lua_concurrent() {
    let store = Arc::new(new_test_store().await);
    let root = store.root_ino();

    let file1 = store
        .create_file(root, "file1.txt".to_string())
        .await
        .unwrap();
    let file2 = store
        .create_file(root, "file2.txt".to_string())
        .await
        .unwrap();

    let mut handles = vec![];
    for _ in 0..4 {
        let store_clone = Arc::clone(&store);
        let handle = tokio::spawn(async move {
            store_clone
                .rename_exchange(root, "file1.txt", root, "file2.txt")
                .await
        });
        handles.push(handle);
    }

    let results: Vec<_> = futures::future::join_all(handles)
        .await
        .into_iter()
        .map(|r| r.unwrap())
        .collect();

    let successes = results.iter().filter(|r| r.is_ok()).count();
    assert_eq!(successes, 4, "all exchanges should succeed (idempotent)");

    let lookup1 = store.lookup(root, "file1.txt").await.unwrap();
    let lookup2 = store.lookup(root, "file2.txt").await.unwrap();
    assert!(
        (lookup1 == Some(file1) && lookup2 == Some(file2))
            || (lookup1 == Some(file2) && lookup2 == Some(file1)),
        "entries should be exchanged or restored to original"
    );
}

#[serial]
#[tokio::test]
#[ignore]
async fn test_rename_exchange_lua_old_not_found() {
    let store = new_test_store().await;
    let root = store.root_ino();

    store
        .create_file(root, "file2.txt".to_string())
        .await
        .unwrap();

    let result = store
        .rename_exchange(root, "nonexistent.txt", root, "file2.txt")
        .await;

    assert!(result.is_err());
    if let Err(MetaError::Internal(msg)) = result {
        assert!(
            msg.contains("Entry 'nonexistent.txt' not found in parent")
                && msg.contains("for exchange"),
            "error message should match format: got '{}'",
            msg
        );
    } else {
        panic!("expected Internal error");
    }
}

#[serial]
#[tokio::test]
#[ignore]
async fn test_rename_exchange_lua_new_not_found() {
    let store = new_test_store().await;
    let root = store.root_ino();

    store
        .create_file(root, "file1.txt".to_string())
        .await
        .unwrap();

    let result = store
        .rename_exchange(root, "file1.txt", root, "nonexistent.txt")
        .await;

    assert!(result.is_err());
    if let Err(MetaError::Internal(msg)) = result {
        assert!(
            msg.contains("Entry 'nonexistent.txt' not found in parent")
                && msg.contains("for exchange"),
            "error message should match format: got '{}'",
            msg
        );
    } else {
        panic!("expected Internal error");
    }
}

#[serial]
#[tokio::test]
#[ignore]
async fn test_rename_exchange_lua_same_entry() {
    let store = new_test_store().await;
    let root = store.root_ino();

    let file_ino = store
        .create_file(root, "file.txt".to_string())
        .await
        .unwrap();
    let node_before = store.get_node(file_ino).await.unwrap().unwrap();

    let result = store
        .rename_exchange(root, "file.txt", root, "file.txt")
        .await;
    assert!(result.is_ok(), "self-exchange should be no-op");

    let node_after = store.get_node(file_ino).await.unwrap().unwrap();
    assert_eq!(
        node_before.attr.mtime, node_after.attr.mtime,
        "mtime should not change"
    );
    assert_eq!(
        node_before.attr.ctime, node_after.attr.ctime,
        "ctime should not change"
    );
}

#[serial]
#[tokio::test]
#[ignore]
async fn test_rename_exchange_lua_hardlinks() {
    let store = new_test_store().await;
    let root = store.root_ino();

    let file1 = store
        .create_file(root, "file1.txt".to_string())
        .await
        .unwrap();
    store.link(file1, root, "link1.txt").await.unwrap();

    let file2 = store
        .create_file(root, "file2.txt".to_string())
        .await
        .unwrap();
    store.link(file2, root, "link2.txt").await.unwrap();

    let node1_before = store.get_node(file1).await.unwrap().unwrap();
    assert_eq!(node1_before.attr.nlink, 2);
    assert_eq!(node1_before.parent, 0);
    assert_eq!(node1_before.name, "");

    let node2_before = store.get_node(file2).await.unwrap().unwrap();
    assert_eq!(node2_before.attr.nlink, 2);
    assert_eq!(node2_before.parent, 0);
    assert_eq!(node2_before.name, "");

    let result = store
        .rename_exchange(root, "file1.txt", root, "file2.txt")
        .await;
    assert!(result.is_ok());

    let link_parents1 = store.load_link_parents(file1).await.unwrap();
    assert_eq!(link_parents1.len(), 2);
    assert!(link_parents1.contains(&(root, "file2.txt".to_string())));
    assert!(link_parents1.contains(&(root, "link1.txt".to_string())));
    assert!(!link_parents1.contains(&(root, "file1.txt".to_string())));

    let link_parents2 = store.load_link_parents(file2).await.unwrap();
    assert_eq!(link_parents2.len(), 2);
    assert!(link_parents2.contains(&(root, "file1.txt".to_string())));
    assert!(link_parents2.contains(&(root, "link2.txt".to_string())));
    assert!(!link_parents2.contains(&(root, "file2.txt".to_string())));

    let node1_after = store.get_node(file1).await.unwrap().unwrap();
    assert_eq!(node1_after.attr.nlink, 2);
    assert_eq!(node1_after.parent, 0);
    assert_eq!(node1_after.name, "");

    let node2_after = store.get_node(file2).await.unwrap().unwrap();
    assert_eq!(node2_after.attr.nlink, 2);
    assert_eq!(node2_after.parent, 0);
    assert_eq!(node2_after.name, "");
}

#[test]
fn test_deserialize_i64_from_number() {
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct TestStruct {
        #[serde(deserialize_with = "super::deserialize_i64_from_number")]
        value: i64,
    }

    // Integer input (normal case)
    let json = r#"{"value": 1234567890}"#;
    let result: TestStruct = serde_json::from_str(json).unwrap();
    assert_eq!(result.value, 1234567890);

    // Float input (the bug case - scientific notation)
    let json = r#"{"value": 1.7698324007242e+18}"#;
    let result: TestStruct = serde_json::from_str(json).unwrap();
    assert!(result.value > 1_700_000_000_000_000_000); // ~1.77e18

    // Negative value
    let json = r#"{"value": -1000}"#;
    let result: TestStruct = serde_json::from_str(json).unwrap();
    assert_eq!(result.value, -1000);

    // Zero
    let json = r#"{"value": 0}"#;
    let result: TestStruct = serde_json::from_str(json).unwrap();
    assert_eq!(result.value, 0);
}

#[serial]
#[tokio::test]
#[ignore]
async fn test_truncate_rewrite_is_atomic_for_partial_chunk() {
    let store = new_test_store().await;
    let root = store.root_ino();
    let ino = store
        .create_file(root, "truncate_race.bin".to_string())
        .await
        .unwrap();
    let chunk_size = 8 * 1024u64;
    let chunk_id = crate::vfs::chunk_id_for(ino, 0).unwrap();

    let base = [
        crate::chunk::SliceDesc {
            slice_id: 11,
            chunk_id,
            offset: 0,
            length: chunk_size,
        },
        crate::chunk::SliceDesc {
            slice_id: 12,
            chunk_id,
            offset: 0,
            length: chunk_size,
        },
    ];
    for desc in base {
        store.append_slice(chunk_id, desc).await.unwrap();
    }
    store.extend_file_size(ino, chunk_size).await.unwrap();

    store.truncate(ino, 1024, chunk_size).await.unwrap();
    let after = store.get_slices(chunk_id).await.unwrap();
    assert_eq!(after.len(), 2);
    assert!(after.iter().all(|s| s.length == 1024));
}

#[serial]
#[tokio::test]
#[ignore]
async fn test_truncate_rewrite_never_exposes_empty_chunk_list() {
    use tokio::task::yield_now;

    let store = Arc::new(new_test_store().await);
    let root = store.root_ino();
    let ino = store
        .create_file(root, "truncate_visibility.bin".to_string())
        .await
        .unwrap();
    let chunk_size = 8 * 1024u64;
    let chunk_id = crate::vfs::chunk_id_for(ino, 0).unwrap();

    store
        .append_slice(
            chunk_id,
            crate::chunk::SliceDesc {
                slice_id: 21,
                chunk_id,
                offset: 0,
                length: chunk_size,
            },
        )
        .await
        .unwrap();
    store.extend_file_size(ino, chunk_size).await.unwrap();

    let writer = store.clone();
    let trunc = tokio::spawn(async move {
        writer.truncate(ino, 1024, chunk_size).await.unwrap();
    });

    let reader = store.clone();
    let observer = tokio::spawn(async move {
        for _ in 0..512 {
            let slices = reader.get_slices(chunk_id).await.unwrap();
            if slices.is_empty() {
                return true;
            }
            yield_now().await;
        }
        false
    });

    trunc.await.unwrap();
    let saw_empty = observer.await.unwrap();
    assert!(!saw_empty, "truncate rewrite exposed an empty slice list");
}

#[serial]
#[tokio::test]
#[ignore]
async fn test_file_default_mode() {
    let store = new_test_store().await;
    let parent = store.root_ino();
    let ino = store
        .create_file(parent, "perm_file.txt".to_string())
        .await
        .unwrap();

    let attr = store.stat(ino).await.unwrap().unwrap();
    assert_eq!(attr.mode & 0o777, 0o644);
}

#[serial]
#[tokio::test]
#[ignore]
async fn test_directory_default_mode() {
    let store = new_test_store().await;
    let parent = store.root_ino();
    let ino = store.mkdir(parent, "perm_dir".to_string()).await.unwrap();

    let attr = store.stat(ino).await.unwrap().unwrap();
    assert_eq!(attr.mode & 0o7777, 0o755);
}

#[serial]
#[tokio::test]
#[ignore]
async fn test_chmod_updates_mode() {
    let store = new_test_store().await;
    let parent = store.root_ino();
    let ino = store
        .create_file(parent, "chmod_test.txt".to_string())
        .await
        .unwrap();

    let attr = store.chmod(ino, 0o755).await.unwrap();
    assert_eq!(attr.mode & 0o777, 0o755);

    let stat = store.stat(ino).await.unwrap().unwrap();
    assert_eq!(stat.mode & 0o777, 0o755);
}

#[serial]
#[tokio::test]
#[ignore]
async fn test_set_attr_mode_strips_special_bits() {
    let store = new_test_store().await;
    let parent = store.root_ino();
    let ino = store
        .create_file(parent, "special_bits.txt".to_string())
        .await
        .unwrap();

    let req = SetAttrRequest {
        mode: Some(0o4755),
        ..Default::default()
    };
    let attr = store
        .set_attr(ino, &req, SetAttrFlags::empty())
        .await
        .unwrap();
    assert_eq!(attr.mode & 0o7777, 0o755);

    let stat = store.stat(ino).await.unwrap().unwrap();
    assert_eq!(stat.mode & 0o7777, 0o755);
}

#[serial]
#[tokio::test]
#[ignore]
async fn test_chmod_nonexistent_inode() {
    let store = new_test_store().await;
    let result = store.chmod(999999, 0o644).await;
    assert!(result.is_err(), "chmod on nonexistent inode should fail");
}
