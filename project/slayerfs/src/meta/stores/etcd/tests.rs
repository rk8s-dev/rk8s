use crate::meta::MetaStore;
use crate::meta::config::Config;
use crate::meta::config::{CacheConfig, ClientOptions, DatabaseConfig, DatabaseType};
use crate::meta::file_lock::{FileLockQuery, FileLockRange, FileLockType};
use crate::meta::store::{MetaError, SetAttrFlags, SetAttrRequest};
use crate::meta::stores::EtcdMetaStore;
use serial_test::serial;
use tokio::time;
use uuid::Uuid;

async fn cleanup_test_data() -> Result<(), MetaError> {
    use etcd_client::GetOptions;

    let mut client = crate::meta::stores::etcd::EtcdClient::connect(vec!["127.0.0.1:2379"], None)
        .await
        .map_err(|e| MetaError::Config(format!("Failed to connect to etcd: {}", e)))?;

    let resp = client
        .get("", Some(GetOptions::new().with_prefix()))
        .await
        .map_err(|e| MetaError::Internal(format!("Failed to get etcd keys: {}", e)))?;

    for kv in resp.kvs() {
        let key = String::from_utf8_lossy(kv.key());
        client
            .delete(key.as_ref(), None)
            .await
            .map_err(|e| MetaError::Internal(format!("Failed to delete key {}: {}", key, e)))?;
    }

    let config = test_config();
    let _store = EtcdMetaStore::from_config(config.clone())
        .await
        .map_err(|e| MetaError::Internal(format!("Failed to reinitialize root: {}", e)))?;

    Ok(())
}

fn test_config() -> Config {
    Config {
        database: DatabaseConfig {
            db_config: DatabaseType::Etcd {
                urls: vec!["127.0.0.1:2379".to_string()],
            },
        },
        cache: CacheConfig::default(),
        client: ClientOptions::default(),
    }
}

/// Configuration for shared database testing (multi-session)
fn shared_db_config() -> Config {
    Config {
        database: DatabaseConfig {
            db_config: DatabaseType::Etcd {
                urls: vec!["127.0.0.1:2379".to_string()],
            },
        },
        cache: CacheConfig::default(),
        client: ClientOptions::default(),
    }
}

async fn new_test_store() -> EtcdMetaStore {
    if let Err(e) = cleanup_test_data().await {
        eprintln!("Failed to cleanup etcd test data: {}", e);
    }

    EtcdMetaStore::from_config(test_config())
        .await
        .expect("Failed to create test database store")
}

/// Create a new test store with pre-configured session ID
async fn new_test_store_with_session(session_id: Uuid) -> EtcdMetaStore {
    let store = new_test_store().await;
    store.set_sid(session_id).expect("Failed to set session ID");
    store
}

/// Helper struct to manage multiple test sessions
struct TestSessionManager {
    stores: Vec<EtcdMetaStore>,
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
        let first_store = EtcdMetaStore::from_config(config.clone())
            .await
            .expect("Failed to create shared test database store");

        let first_session_id = Uuid::now_v7();
        first_store
            .set_sid(first_session_id)
            .expect("Failed to set session ID");

        stores.push(first_store);
        session_ids.push(first_session_id);

        for _ in 1..session_count {
            let store = EtcdMetaStore::from_config(config.clone())
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

    fn get_store(&self, index: usize) -> &EtcdMetaStore {
        &self.stores[index]
    }
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
