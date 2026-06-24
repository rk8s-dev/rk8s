use crate::chunk::SliceDesc;
use crate::meta::MetaStore;
use crate::meta::config::Config;
use crate::meta::config::{
    CacheConfig, ClientOptions, CompactConfig, DatabaseConfig, DatabaseType,
};
use crate::meta::file_lock::{FileLockQuery, FileLockRange, FileLockType};
use crate::meta::store::{LockName, MetaError, SetAttrFlags, SetAttrRequest};
use crate::meta::stores::EtcdMetaStore;
use crate::vfs::chunk_id_for;
use chrono::Utc;
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
        compact: CompactConfig::default(),
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
        compact: CompactConfig::default(),
    }
}

async fn new_test_store() -> EtcdMetaStore {
    if let Err(e) = cleanup_test_data().await {
        eprintln!("Failed to cleanup etcd test data: {}", e);
    }

    EtcdMetaStore::from_config(test_config())
        .await
        .expect("Failed to create test etcd store")
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
            .expect("Failed to create shared test etcd store");

        let first_session_id = Uuid::now_v7();
        first_store
            .set_sid(first_session_id)
            .expect("Failed to set session ID");

        stores.push(first_store);
        session_ids.push(first_session_id);

        for _ in 1..session_count {
            let store = EtcdMetaStore::from_config(config.clone())
                .await
                .expect("Failed to create shared test etcd store");

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
async fn test_directory_same_parent_rename_updates_lookup() {
    let store = new_test_store().await;
    let root = store.root_ino();

    let parent = store.mkdir(root, "parent".to_string()).await.unwrap();
    let dir_ino = store.mkdir(parent, "old_dir".to_string()).await.unwrap();

    store
        .rename(parent, "old_dir", parent, "new_dir".to_string())
        .await
        .unwrap();

    assert_eq!(store.lookup(parent, "old_dir").await.unwrap(), None);
    assert_eq!(
        store.lookup(parent, "new_dir").await.unwrap(),
        Some(dir_ino)
    );
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

#[serial]
#[tokio::test]
#[ignore]
async fn test_compaction_gc_roundtrip_etcd() {
    let store = new_test_store().await;

    store
        .append_slice(
            11,
            SliceDesc {
                slice_id: 101,
                chunk_id: 11,
                offset: 0,
                length: 4096,
            },
        )
        .await
        .unwrap();
    store
        .append_slice(
            22,
            SliceDesc {
                slice_id: 202,
                chunk_id: 22,
                offset: 0,
                length: 1024,
            },
        )
        .await
        .unwrap();

    assert_eq!(store.list_chunk_ids(10).await.unwrap(), vec![11, 22]);
    assert_eq!(store.list_chunk_ids(1).await.unwrap().len(), 1);

    let current = store.get_slices(11).await.unwrap();
    let delayed = SliceDesc::encode_delayed_data(&current, &[101]);
    let replacement = SliceDesc {
        slice_id: 303,
        chunk_id: 11,
        offset: 0,
        length: 2048,
    };

    store
        .replace_slices_for_compact(11, &[replacement], &delayed)
        .await
        .unwrap();

    assert_eq!(store.get_slices(11).await.unwrap(), vec![replacement]);

    let pending = store.process_delayed_slices(10, -1).await.unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].0, 101);
    assert_eq!(pending[0].1, 0);
    assert_eq!(pending[0].2, 4096);

    store
        .confirm_delayed_deleted(&[pending[0].3])
        .await
        .unwrap();
    assert!(
        store
            .process_delayed_slices(10, -1)
            .await
            .unwrap()
            .is_empty()
    );
}

#[serial]
#[tokio::test]
#[ignore]
async fn test_light_compaction_preserves_unreplaced_slices_etcd() {
    let store = new_test_store().await;

    let retained = SliceDesc {
        slice_id: 501,
        chunk_id: 55,
        offset: 0,
        length: 4096,
    };
    let replaced = SliceDesc {
        slice_id: 502,
        chunk_id: 55,
        offset: 1024,
        length: 1024,
    };

    store.append_slice(55, retained).await.unwrap();
    store.append_slice(55, replaced).await.unwrap();

    let current = store.get_slices(55).await.unwrap();
    let delayed = SliceDesc::encode_delayed_data(&current, &[502]);

    store
        .replace_slices_for_compact(55, &[], &delayed)
        .await
        .unwrap();

    assert_eq!(store.get_slices(55).await.unwrap(), vec![retained]);

    let pending = store.process_delayed_slices(10, -1).await.unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].0, 502);
    assert_eq!(pending[0].1, 1024);
    assert_eq!(pending[0].2, 1024);
}

#[serial]
#[tokio::test]
#[ignore]
async fn test_process_delayed_slices_removes_empty_chunk_key_etcd() {
    let store = new_test_store().await;

    let original = SliceDesc {
        slice_id: 601,
        chunk_id: 66,
        offset: 0,
        length: 512,
    };
    store.append_slice(66, original).await.unwrap();

    let delayed = SliceDesc::encode_delayed_data(&[original], &[601]);
    store
        .replace_slices_for_compact(66, &[], &delayed)
        .await
        .unwrap();

    assert!(store.get_slices(66).await.unwrap().is_empty());
    assert_eq!(store.list_chunk_ids(10).await.unwrap(), Vec::<u64>::new());

    let pending = store.process_delayed_slices(10, -1).await.unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].0, 601);

    assert!(store.get_slices(66).await.unwrap().is_empty());
    assert_eq!(store.list_chunk_ids(10).await.unwrap(), Vec::<u64>::new());
}

#[serial]
#[tokio::test]
#[ignore]
async fn test_uncommitted_gc_and_version_conflict_etcd() {
    let store = new_test_store().await;

    let initial = SliceDesc {
        slice_id: 401,
        chunk_id: 33,
        offset: 0,
        length: 512,
    };
    store.append_slice(33, initial).await.unwrap();

    let replacement = SliceDesc {
        slice_id: 402,
        chunk_id: 33,
        offset: 0,
        length: 256,
    };
    let delayed = SliceDesc::encode_delayed_data(&[initial], &[401]);

    let err = store
        .replace_slices_for_compact_with_version(33, &[replacement], &delayed, &[])
        .await
        .unwrap_err();
    assert!(matches!(err, MetaError::ContinueRetry));

    store
        .record_uncommitted_slice(402, 33, 256, "compact_heavy")
        .await
        .unwrap();
    assert!(
        store
            .etcd_get_json_serde_only::<serde_json::Value>("gc/uncommitted/pending/402")
            .await
            .unwrap()
            .is_some()
    );

    store
        .replace_slices_for_compact_with_version(33, &[replacement], &delayed, &[initial])
        .await
        .unwrap();
    assert_eq!(store.get_slices(33).await.unwrap(), vec![replacement]);
    assert!(
        store
            .etcd_get_json_serde_only::<serde_json::Value>("gc/uncommitted/pending/402")
            .await
            .unwrap()
            .is_none()
    );

    let first_id = store
        .record_uncommitted_slice(9001, 33, 8192, "compact_heavy")
        .await
        .unwrap();
    assert!(first_id > 0);

    let orphans = store
        .cleanup_orphan_uncommitted_slices(-1, 10)
        .await
        .unwrap();
    assert_eq!(orphans, vec![(9001, 8192)]);

    store.delete_uncommitted_slices(&[9001]).await.unwrap();
    assert!(
        store
            .cleanup_orphan_uncommitted_slices(-1, 10)
            .await
            .unwrap()
            .is_empty()
    );

    let second_id = store
        .record_uncommitted_slice(9002, 33, 4096, "compact_heavy")
        .await
        .unwrap();
    assert!(second_id > first_id);
    store.confirm_slice_committed(9002).await.unwrap();
    assert!(
        store
            .cleanup_orphan_uncommitted_slices(-1, 10)
            .await
            .unwrap()
            .is_empty()
    );
}

#[serial]
#[tokio::test]
#[ignore]
async fn test_chunk_compact_lock_blocks_write_until_release_etcd() {
    let store = new_test_store().await;
    let ino = store
        .create_file(store.root_ino(), "lock_write.txt".to_string())
        .await
        .unwrap();

    assert!(
        store
            .get_global_lock(LockName::ChunkCompactLock(44), 30)
            .await
    );
    assert!(
        store
            .is_global_lock_held(LockName::ChunkCompactLock(44), 30)
            .await
    );

    let blocked = store
        .write(
            ino,
            44,
            SliceDesc {
                slice_id: 7001,
                chunk_id: 44,
                offset: 0,
                length: 128,
            },
            128,
        )
        .await
        .unwrap_err();
    assert!(matches!(blocked, MetaError::ContinueRetry));

    assert!(
        store
            .release_global_lock(LockName::ChunkCompactLock(44))
            .await
    );
    assert!(
        !store
            .is_global_lock_held(LockName::ChunkCompactLock(44), 30)
            .await
    );

    store
        .write(
            ino,
            44,
            SliceDesc {
                slice_id: 7002,
                chunk_id: 44,
                offset: 0,
                length: 128,
            },
            128,
        )
        .await
        .unwrap();

    let slices = store.get_slices(44).await.unwrap();
    assert_eq!(slices.len(), 1);
    assert_eq!(slices[0].slice_id, 7002);
}

#[serial]
#[tokio::test]
#[ignore]
async fn test_stale_compact_lock_release_does_not_delete_reacquired_lock_etcd() {
    let stale_holder = new_test_store().await;
    let current_holder = EtcdMetaStore::from_config(test_config())
        .await
        .expect("Failed to create second etcd store");

    assert!(
        stale_holder
            .get_global_lock(LockName::ChunkCompactLock(77), 0)
            .await
    );
    time::sleep(std::time::Duration::from_millis(2)).await;

    assert!(
        current_holder
            .get_global_lock(LockName::ChunkCompactLock(77), 0)
            .await
    );

    assert!(
        !stale_holder
            .release_global_lock(LockName::ChunkCompactLock(77))
            .await
    );
    assert!(
        current_holder
            .is_global_lock_held(LockName::ChunkCompactLock(77), 30)
            .await
    );

    assert!(
        current_holder
            .release_global_lock(LockName::ChunkCompactLock(77))
            .await
    );
    assert!(
        !current_holder
            .is_global_lock_held(LockName::ChunkCompactLock(77), 30)
            .await
    );
}

#[serial]
#[tokio::test]
#[ignore]
async fn test_expired_compact_lock_does_not_block_write_etcd() {
    let store = new_test_store().await;
    let ino = store
        .create_file(store.root_ino(), "expired_lock_write.txt".to_string())
        .await
        .unwrap();
    let expired_at =
        chrono::Utc::now().timestamp_millis() - chrono::Duration::days(1).num_milliseconds();

    store
        .etcd_put_json_serde_only(
            LockName::ChunkCompactLock(91).to_string(),
            &expired_at,
            None,
        )
        .await
        .unwrap();

    store
        .write(
            ino,
            91,
            SliceDesc {
                slice_id: 7101,
                chunk_id: 91,
                offset: 0,
                length: 128,
            },
            128,
        )
        .await
        .unwrap();

    let slices = store.get_slices(91).await.unwrap();
    assert_eq!(slices.len(), 1);
    assert_eq!(slices[0].slice_id, 7101);
}

#[serial]
#[tokio::test]
#[ignore]
async fn test_truncate_updates_size_and_prunes_slices_etcd() {
    let store = new_test_store().await;
    let ino = store
        .create_file(store.root_ino(), "truncate.txt".to_string())
        .await
        .unwrap();
    let chunk_size = 1024;
    let chunk0 = chunk_id_for(ino, 0).unwrap();
    let chunk1 = chunk_id_for(ino, 1).unwrap();

    store
        .write(
            ino,
            chunk0,
            SliceDesc {
                slice_id: 8001,
                chunk_id: chunk0,
                offset: 0,
                length: 1024,
            },
            2048,
        )
        .await
        .unwrap();
    store
        .write(
            ino,
            chunk1,
            SliceDesc {
                slice_id: 8002,
                chunk_id: chunk1,
                offset: 0,
                length: 1024,
            },
            2048,
        )
        .await
        .unwrap();

    store.truncate(ino, 512, chunk_size).await.unwrap();

    assert_eq!(store.stat(ino).await.unwrap().unwrap().size, 512);
    assert_eq!(
        store.get_slices(chunk0).await.unwrap(),
        vec![SliceDesc {
            slice_id: 8001,
            chunk_id: chunk0,
            offset: 0,
            length: 512,
        }]
    );
    assert!(store.get_slices(chunk1).await.unwrap().is_empty());
}

#[serial]
#[tokio::test]
#[ignore]
async fn test_truncate_batches_large_chunk_deletes_etcd() {
    let store = new_test_store().await;
    let ino = store
        .create_file(store.root_ino(), "truncate_large.txt".to_string())
        .await
        .unwrap();
    let chunk_size = 1024;
    let chunk_count = 60;

    for idx in 0..chunk_count {
        let chunk_id = chunk_id_for(ino, idx).unwrap();
        store
            .write(
                ino,
                chunk_id,
                SliceDesc {
                    slice_id: 9000 + idx,
                    chunk_id,
                    offset: 0,
                    length: chunk_size,
                },
                (idx + 1) * chunk_size,
            )
            .await
            .unwrap();
    }

    store.truncate(ino, 0, chunk_size).await.unwrap();

    assert_eq!(store.stat(ino).await.unwrap().unwrap().size, 0);
    for idx in 0..chunk_count {
        let chunk_id = chunk_id_for(ino, idx).unwrap();
        assert!(store.get_slices(chunk_id).await.unwrap().is_empty());
    }
}

#[serial]
#[tokio::test]
#[ignore]
async fn test_replace_slices_for_compact_rejects_non_atomic_large_delayed_batch_etcd() {
    let store = new_test_store().await;
    let chunk_id = 88;
    let slice_count = 60;
    let mut slices = Vec::new();
    let mut replaced_ids = Vec::new();

    for idx in 0..slice_count {
        let slice = SliceDesc {
            slice_id: 10_000 + idx,
            chunk_id,
            offset: idx * 1024,
            length: 1024,
        };
        store.append_slice(chunk_id, slice).await.unwrap();
        slices.push(slice);
        replaced_ids.push(slice.slice_id);
    }

    let delayed = SliceDesc::encode_delayed_data(&slices, &replaced_ids);
    let err = store
        .replace_slices_for_compact(chunk_id, &[], &delayed)
        .await
        .unwrap_err();
    assert!(
        matches!(err, MetaError::Internal(message) if message.contains("Atomic compaction requires"))
    );

    assert_eq!(store.get_slices(chunk_id).await.unwrap(), slices);
    assert!(
        store
            .process_delayed_slices(10, -1)
            .await
            .unwrap()
            .is_empty()
    );
}

#[serial]
#[tokio::test]
#[ignore]
async fn test_cleanup_orphan_uncommitted_slices_pages_and_keeps_oldest_ids_etcd() {
    let store = new_test_store().await;
    let chunk_id = 89;

    for idx in 0..130u64 {
        store
            .record_uncommitted_slice(20_000 + idx, chunk_id, 512 + idx, "compact_heavy")
            .await
            .unwrap();
    }

    let cleaned = store
        .cleanup_orphan_uncommitted_slices(-1, 3)
        .await
        .unwrap();
    assert_eq!(cleaned, vec![(20_000, 512), (20_001, 513), (20_002, 514)]);
}

#[serial]
#[tokio::test]
#[ignore]
async fn test_list_chunk_ids_rotates_scan_window_etcd() {
    let store = new_test_store().await;

    for chunk_id in [1u64, 2, 3, 4] {
        store
            .append_slice(
                chunk_id,
                SliceDesc {
                    slice_id: 30_000 + chunk_id,
                    chunk_id,
                    offset: 0,
                    length: 512,
                },
            )
            .await
            .unwrap();
    }

    assert_eq!(store.list_chunk_ids(2).await.unwrap(), vec![1, 2]);
    assert_eq!(store.list_chunk_ids(2).await.unwrap(), vec![3, 4]);
    assert_eq!(store.list_chunk_ids(2).await.unwrap(), vec![1, 2]);
}

#[serial]
#[tokio::test]
#[ignore]
async fn test_list_chunk_ids_does_not_duplicate_short_prefix_scan_etcd() {
    let store = new_test_store().await;

    for chunk_id in [1u64, 2, 3, 4] {
        store
            .append_slice(
                chunk_id,
                SliceDesc {
                    slice_id: 31_000 + chunk_id,
                    chunk_id,
                    offset: 0,
                    length: 512,
                },
            )
            .await
            .unwrap();
    }

    assert_eq!(store.list_chunk_ids(10).await.unwrap(), vec![1, 2, 3, 4]);
    assert_eq!(store.list_chunk_ids(10).await.unwrap(), vec![1, 2, 3, 4]);
}

#[serial]
#[tokio::test]
#[ignore]
async fn test_process_delayed_slices_filters_by_age_before_limit_etcd() {
    let store = new_test_store().await;
    let chunk_id = 90;

    for idx in 0..6u64 {
        let slice = SliceDesc {
            slice_id: 40_000 + idx,
            chunk_id,
            offset: idx * 10,
            length: 64,
        };
        store.append_slice(chunk_id, slice).await.unwrap();
        let delayed = SliceDesc::encode_delayed_data(&[slice], &[slice.slice_id]);
        store
            .replace_slices_for_compact(chunk_id, &[], &delayed)
            .await
            .unwrap();
    }

    let stale_pending_keys = [
        "gc/delayed/pending/1",
        "gc/delayed/pending/2",
        "gc/delayed/pending/3",
    ];
    let recent_pending_keys = [
        "gc/delayed/pending/4",
        "gc/delayed/pending/5",
        "gc/delayed/pending/6",
    ];

    let now = Utc::now().timestamp();
    let stale_created_at = now - 7200;
    let recent_created_at = now;

    for key in stale_pending_keys {
        let mut record = store
            .etcd_get_json_serde_only::<serde_json::Value>(key)
            .await
            .unwrap()
            .unwrap();
        record["created_at"] = serde_json::Value::from(stale_created_at);
        store
            .etcd_put_json_serde_only(key, &record, None)
            .await
            .unwrap();
    }

    for key in recent_pending_keys {
        let mut record = store
            .etcd_get_json_serde_only::<serde_json::Value>(key)
            .await
            .unwrap()
            .unwrap();
        record["created_at"] = serde_json::Value::from(recent_created_at);
        store
            .etcd_put_json_serde_only(key, &record, None)
            .await
            .unwrap();
    }

    let ready = store.process_delayed_slices(3, 3600).await.unwrap();
    assert_eq!(ready.len(), 3);
    assert_eq!(ready[0].0, 40_000);
    assert_eq!(ready[1].0, 40_001);
    assert_eq!(ready[2].0, 40_002);

    let processed_ids: Vec<i64> = ready.iter().map(|entry| entry.3).collect();
    store.confirm_delayed_deleted(&processed_ids).await.unwrap();

    assert!(
        store
            .process_delayed_slices(10, 3600)
            .await
            .unwrap()
            .is_empty()
    );
}
