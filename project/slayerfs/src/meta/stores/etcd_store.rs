//! Etcd-based metadata store implementation
//!
//! Uses Etcd/etcd as the backend for metadata storage

use crate::chuck::SliceDesc;
use crate::chuck::slice::key_for_slice;
use crate::meta::backoff::backoff;
use crate::meta::client::session::{Session, SessionInfo};
use crate::meta::config::{Config, DatabaseType};
use crate::meta::entities::etcd::EtcdLinkParent;
use crate::meta::entities::etcd::*;
use crate::meta::entities::*;
use crate::meta::file_lock::{
    FileLockInfo, FileLockQuery, FileLockRange, FileLockType, PlockRecord,
};
use crate::meta::store::{
    DirEntry, FileAttr, LockName, MetaError, MetaStore, SetAttrFlags, SetAttrRequest,
};
use crate::meta::stores::pool::IdPool;
use crate::meta::{INODE_ID_KEY, Permission};
use crate::vfs::fs::FileType;
use async_trait::async_trait;
use chrono::{Duration, Utc};
use etcd_client::{Client as EtcdClient, Compare, CompareOp, LeaseKeeper, PutOptions, Txn, TxnOp};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json;
use std::collections::HashMap;
use std::path::Path;
use std::sync::OnceLock;
use tokio::select;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};
use uuid::Uuid;

/// ID allocation batch size
/// TODO: make configurable.
const BATCH_SIZE: i64 = 1000;
const FIRST_ALLOCATED_ID: i64 = 2;

/// Etcd-based metadata store
pub struct EtcdMetaStore {
    client: EtcdClient,
    _config: Config,
    /// Local ID pools keyed by counter key (inode, slice, etc.)
    id_pools: IdPool,
    sid: OnceLock<Uuid>,
    lease: OnceLock<i64>,
}

#[allow(dead_code)]
impl EtcdMetaStore {
    /// Etcd helper method: generate forward index key (parent_inode, name)
    fn etcd_forward_key(parent_inode: i64, name: &str) -> String {
        format!("f:{}:{}", parent_inode, name)
    }

    /// Etcd helper method: generate reverse index key for inode
    fn etcd_reverse_key(ino: i64) -> String {
        format!("r:{}", ino)
    }

    /// Etcd helper method: generate directory children key
    fn etcd_children_key(inode: i64) -> String {
        format!("c:{}", inode)
    }

    fn etcd_session_key(session_id: Option<Uuid>) -> String {
        match session_id {
            Some(id) => format!("session:{}", id),
            None => "session:".to_string(),
        }
    }

    fn etcd_session_info_key(session_id: Option<Uuid>) -> String {
        match session_id {
            Some(id) => format!("session_info:{}", id),
            None => "session_info:".to_string(),
        }
    }

    fn get_session_id_from_session_key(session_key: &str) -> Option<Uuid> {
        session_key
            .strip_prefix("session:")
            .and_then(|s| Uuid::parse_str(s).ok())
    }

    fn etcd_plock_key(inode: i64) -> String {
        format!("p:{inode}")
    }

    /// Etcd helper method: generate link parent key for multi-hardlink files
    fn etcd_link_parent_key(inode: i64) -> String {
        format!("l:{}", inode)
    }

    /// Create or open an etcd metadata store
    pub async fn new(backend_path: &Path) -> Result<Self, MetaError> {
        let _config =
            Config::from_path(backend_path).map_err(|e| MetaError::Config(e.to_string()))?;

        info!("Initializing EtcdMetaStore");
        info!("Backend path: {}", backend_path.display());

        let client = Self::create_client(&_config).await?;
        let store = Self {
            client,
            _config,
            id_pools: IdPool::default(),
            sid: OnceLock::new(),
            lease: OnceLock::new(),
        };
        store.init_root_directory().await?;

        info!("EtcdMetaStore initialized successfully");
        Ok(store)
    }

    /// Create from existing config
    pub async fn from_config(_config: Config) -> Result<Self, MetaError> {
        info!("Initializing EtcdMetaStore from config");

        let client = Self::create_client(&_config).await?;
        let store = Self {
            client,
            _config,
            id_pools: IdPool::default(),
            sid: OnceLock::new(),
            lease: OnceLock::new(),
        };
        store.init_root_directory().await?;

        info!("EtcdMetaStore initialized successfully");
        Ok(store)
    }

    /// Create etcd client
    async fn create_client(config: &Config) -> Result<EtcdClient, MetaError> {
        match &config.database.db_config {
            DatabaseType::Etcd { urls } => {
                info!("Connecting to Etcd cluster: {:?}", urls);
                let client = EtcdClient::connect(urls, None)
                    .await
                    .map_err(|e| MetaError::Config(format!("Failed to connect to Etcd: {}", e)))?;
                Ok(client)
            }
            DatabaseType::Sqlite { .. } | DatabaseType::Postgres { .. } => Err(MetaError::Config(
                "SQL database backend not supported by EtcdMetaStore. Use DatabaseMetaStore instead."
                    .to_string(),
            )),
            DatabaseType::Redis { .. } => Err(MetaError::Config(
                "Redis backend not supported by EtcdMetaStore. Use RedisMetaStore instead."
                    .to_string(),
            )),
        }
    }

    /// Helper: get key from etcd and deserialize JSON into T.
    ///
    /// Strict variant: returns Err(MetaError::Internal) when etcd client returns error.
    async fn etcd_get_json<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>, MetaError> {
        let mut client = self.client.clone();
        match client.get(key.to_string(), None).await {
            Ok(resp) => {
                if let Some(kv) = resp.kvs().first() {
                    let obj: T = serde_json::from_slice(kv.value()).map_err(|e| {
                        MetaError::Internal(format!("Failed to parse {}: {}", key, e))
                    })?;
                    Ok(Some(obj))
                } else {
                    Ok(None)
                }
            }
            Err(e) => Err(MetaError::Internal(format!(
                "Failed to get key {}: {}",
                key, e
            ))),
        }
    }

    async fn etcd_put_json<T: Serialize>(
        &self,
        key: impl AsRef<str>,
        obj: &T,
        options: Option<PutOptions>,
    ) -> Result<(), MetaError> {
        let mut client = self.client.clone();

        let json = serde_json::to_string(obj)?;
        let key = key.as_ref();

        client
            .put(key, json, options)
            .await
            .map(|_| ())
            .map_err(|e| MetaError::Internal(format!("Failed to put key {key}: {e}")))
    }

    /// Lenient variant: on etcd client error, log and return Ok(None).
    async fn etcd_get_json_lenient<T: DeserializeOwned>(
        &self,
        key: &str,
    ) -> Result<Option<T>, MetaError> {
        match self.etcd_get_json::<T>(key).await {
            Ok(v) => Ok(v),
            Err(e) => {
                error!("Etcd get failed for {}: {}", key, e);
                Ok(None)
            }
        }
    }

    /// Initialize root directory
    async fn init_root_directory(&self) -> Result<(), MetaError> {
        let children_key = Self::etcd_children_key(1);
        let root_children = EtcdDirChildren::new(1, HashMap::new());
        let children_json = serde_json::to_string(&root_children)?;

        let mut client = self.client.clone();

        // Atomically create root directory only if it doesn't exist
        // version == 0 means the key is currently not present
        let txn = Txn::new()
            .when([Compare::version(children_key.clone(), CompareOp::Equal, 0)])
            .and_then([TxnOp::put(children_key.clone(), children_json, None)]);

        let resp = client.txn(txn).await.map_err(|e| {
            MetaError::Config(format!("Failed to initialize root directory: {}", e))
        })?;

        if resp.succeeded() {
            info!("Root directory initialized for Etcd backend");
        } else {
            info!("Root directory already exists for Etcd backend");
        }

        Ok(())
    }

    /// Get directory access metadata
    async fn get_access_meta(&self, inode: i64) -> Result<Option<AccessMetaModel>, MetaError> {
        if inode == 1 {
            let now = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0);
            return Ok(Some(AccessMetaModel {
                inode: 1,
                permission: Permission::new(0o40755, 0, 0),
                access_time: now,
                modify_time: now,
                create_time: now,
                nlink: 2,
            }));
        }

        let reverse_key = Self::etcd_reverse_key(inode);
        // lenient: if etcd client fails, treat as not found (caller expects Option)
        if let Ok(Some(entry_info)) = self
            .etcd_get_json_lenient::<EtcdEntryInfo>(&reverse_key)
            .await
            && !entry_info.is_file
        {
            let permission = entry_info.permission().clone();
            let access_meta = AccessMetaModel::from_permission(
                inode,
                permission,
                entry_info.access_time,
                entry_info.modify_time,
                entry_info.create_time,
                entry_info.nlink as i32,
            );
            return Ok(Some(access_meta));
        }
        Ok(None)
    }

    /// Get directory content metadata
    async fn get_content_meta(
        &self,
        parent_inode: i64,
    ) -> Result<Option<Vec<ContentMetaModel>>, MetaError> {
        let children_key = Self::etcd_children_key(parent_inode);
        // strict read of children list
        let dir_children_opt = self
            .etcd_get_json_lenient::<EtcdDirChildren>(&children_key)
            .await?;
        let dir_children = match dir_children_opt {
            Some(dc) => dc,
            None => return Ok(None),
        };

        if dir_children.children.is_empty() {
            return Ok(None);
        }

        // Optimization: Batch fetch all forward entries with a single prefix query
        // Instead of N individual queries, use one range request for f:{parent_inode}:
        let mut client = self.client.clone();
        let forward_prefix = format!("f:{}:", parent_inode);

        let forward_entries_map: HashMap<String, EtcdForwardEntry> = match client
            .get(
                forward_prefix.clone(),
                Some(etcd_client::GetOptions::new().with_prefix()),
            )
            .await
        {
            Ok(resp) => {
                let mut map = HashMap::new();
                for kv in resp.kvs() {
                    if let Ok(entry) = serde_json::from_slice::<EtcdForwardEntry>(kv.value()) {
                        // Extract child name from key: "f:{parent}:{name}" -> name
                        let key_str = String::from_utf8_lossy(kv.key());
                        if let Some(name) = key_str.strip_prefix(&forward_prefix) {
                            map.insert(name.to_string(), entry);
                        }
                    }
                }
                map
            }
            Err(e) => {
                error!(
                    "Failed to batch fetch forward entries for parent_inode {}: {}. Directory will appear empty.",
                    parent_inode, e
                );
                return Err(MetaError::Internal(format!(
                    "Failed to batch fetch forward entries for parent_inode {}: {}",
                    parent_inode, e
                )));
            }
        };

        let mut content_list = Vec::new();
        // Sort children names to ensure consistent order (matching BTreeMap in cache)
        let mut sorted_names: Vec<_> = dir_children.children.keys().collect();
        sorted_names.sort();

        for child_name in sorted_names {
            if let Some(forward_entry) = forward_entries_map.get(child_name.as_str()) {
                let entry_type = forward_entry.resolved_entry_type();

                content_list.push(ContentMetaModel {
                    inode: forward_entry.inode,
                    parent_inode,
                    entry_name: child_name.clone(),
                    entry_type,
                });
            }
        }

        if content_list.is_empty() {
            Ok(None)
        } else {
            Ok(Some(content_list))
        }
    }

    /// Get file metadata
    async fn get_file_meta(&self, inode: i64) -> Result<Option<FileMetaModel>, MetaError> {
        let reverse_key = Self::etcd_reverse_key(inode);
        if let Ok(Some(entry_info)) = self
            .etcd_get_json_lenient::<EtcdEntryInfo>(&reverse_key)
            .await
            && entry_info.is_file
        {
            let permission = entry_info.permission().clone();
            let file_meta = FileMetaModel::from_permission(
                inode,
                entry_info.size.unwrap_or(0),
                permission,
                entry_info.access_time,
                entry_info.modify_time,
                entry_info.create_time,
                entry_info.nlink as i32,
                entry_info.parent_inode,
                entry_info.deleted,
                entry_info.symlink_target.clone(),
            );
            return Ok(Some(file_meta));
        }
        Ok(None)
    }

    /// Create a new directory
    async fn create_directory(&self, parent_inode: i64, name: String) -> Result<i64, MetaError> {
        // Step 1: Verify parent exists and get its metadata
        let parent_meta = self.get_access_meta(parent_inode).await?;
        if parent_meta.is_none() {
            return Err(MetaError::ParentNotFound(parent_inode));
        }
        let parent_meta = parent_meta.unwrap();

        if let Some(contents) = self.get_content_meta(parent_inode).await? {
            for content in contents {
                if content.entry_name == name {
                    return Err(MetaError::AlreadyExists {
                        parent: parent_inode,
                        name,
                    });
                }
            }
        }

        let inode = self.generate_id(INODE_ID_KEY).await?;

        let now = Utc::now().timestamp_nanos_opt().unwrap_or(0);

        // Inherit gid from parent if parent has setgid bit set
        let parent_perm = &parent_meta.permission;
        let parent_has_setgid = (parent_perm.mode & 0o2000) != 0;
        let gid = if parent_has_setgid {
            parent_perm.gid
        } else {
            0
        };

        // Directories inherit setgid bit from parent
        let mode = if parent_has_setgid {
            0o42755 // Directory with setgid bit
        } else {
            0o40755 // Regular directory
        };

        let dir_permission = Permission::new(mode, 0, gid);
        let entry_info = EtcdEntryInfo {
            is_file: false,
            size: None,
            version: None,
            permission: dir_permission,
            access_time: now,
            modify_time: now,
            create_time: now,
            nlink: 2,
            parent_inode,
            entry_name: name.clone(),
            deleted: false,
            symlink_target: None,
        };

        let forward_key = Self::etcd_forward_key(parent_inode, &name);
        let forward_entry = EtcdForwardEntry {
            parent_inode,
            name: name.clone(),
            inode,
            is_file: false,
            entry_type: Some(EntryType::Directory),
        };
        let forward_json = serde_json::to_string(&forward_entry)?;

        let reverse_key = Self::etcd_reverse_key(inode);
        let reverse_json = serde_json::to_string(&entry_info)?;

        let children_key = Self::etcd_children_key(inode);
        let children = EtcdDirChildren::new(inode, HashMap::new());
        let children_json = serde_json::to_string(&children)?;

        // Step 2: Atomic transaction - create all keys only if forward key doesn't exist
        info!(
            "Creating directory with transaction: parent={}, name={}, inode={}",
            parent_inode, name, inode
        );

        let operations = vec![
            (forward_key.as_str(), forward_json.as_str()),
            (reverse_key.as_str(), reverse_json.as_str()),
            (children_key.as_str(), children_json.as_str()),
        ];

        self.create_entry(&forward_key, &operations, parent_inode, &name)
            .await?;

        // Step 3: Update parent's children set
        // If this fails, forward/reverse/children keys are created
        // but parent's children map is not updated. Consider using compensation or
        // background reconciliation.
        let name_for_closure = name.clone();
        let inode_for_closure = inode;
        match self
            .update_parent_children(
                parent_inode,
                move |children| {
                    children.insert(name_for_closure.clone(), inode_for_closure);
                },
                10,
            )
            .await
        {
            Ok(_) => {
                info!(
                    "Directory created successfully: parent={}, name={}, inode={}",
                    parent_inode, name, inode
                );
                Ok(inode)
            }
            Err(e) => {
                // Compensation: Try to rollback the created entry
                error!(
                    "Failed to update parent children for dir creation, attempting rollback: parent={}, name={}, inode={}, error={}",
                    parent_inode, name, inode, e
                );

                let rollback_keys = vec![
                    forward_key.as_str(),
                    reverse_key.as_str(),
                    children_key.as_str(),
                ];

                if let Err(rollback_err) =
                    self.delete_entry(&forward_key, &rollback_keys, inode).await
                {
                    error!(
                        "Failed to rollback directory creation: inode={}, error={}. Manual cleanup may be required.",
                        inode, rollback_err
                    );
                }

                Err(MetaError::Internal(format!(
                    "Failed to create directory: {}",
                    e
                )))
            }
        }
    }

    /// Create a new file
    async fn create_file_internal(
        &self,
        parent_inode: i64,
        name: String,
    ) -> Result<i64, MetaError> {
        // Step 1: Verify parent exists and get its metadata
        let parent_meta = self.get_access_meta(parent_inode).await?;
        if parent_meta.is_none() {
            return Err(MetaError::ParentNotFound(parent_inode));
        }
        let parent_meta = parent_meta.unwrap();

        if let Some(contents) = self.get_content_meta(parent_inode).await? {
            for content in contents {
                if content.entry_name == name {
                    return Err(MetaError::AlreadyExists {
                        parent: parent_inode,
                        name,
                    });
                }
            }
        }

        let inode = self.generate_id(INODE_ID_KEY).await?;

        let now = Utc::now().timestamp_nanos_opt().unwrap_or(0);

        // Inherit gid from parent if parent has setgid bit set
        let parent_perm = &parent_meta.permission;
        let parent_has_setgid = (parent_perm.mode & 0o2000) != 0;
        let gid = if parent_has_setgid {
            parent_perm.gid
        } else {
            0
        };

        let file_permission = Permission::new(0o100644, 0, gid);
        let entry_info = EtcdEntryInfo {
            is_file: true,
            size: Some(0),
            version: Some(0),
            permission: file_permission,
            access_time: now,
            modify_time: now,
            create_time: now,
            nlink: 1,
            parent_inode,
            entry_name: name.clone(),
            deleted: false,
            symlink_target: None,
        };

        let forward_key = Self::etcd_forward_key(parent_inode, &name);
        let forward_entry = EtcdForwardEntry {
            parent_inode,
            name: name.clone(),
            inode,
            is_file: true,
            entry_type: Some(EntryType::File),
        };
        let forward_json = serde_json::to_string(&forward_entry)?;

        let reverse_key = Self::etcd_reverse_key(inode);
        let reverse_json = serde_json::to_string(&entry_info)?;

        // Step 2: Atomic transaction - create keys only if forward key doesn't exist
        info!(
            "Creating file with transaction: parent={}, name={}, inode={}",
            parent_inode, name, inode
        );

        let operations = vec![
            (forward_key.as_str(), forward_json.as_str()),
            (reverse_key.as_str(), reverse_json.as_str()),
        ];

        self.create_entry(&forward_key, &operations, parent_inode, &name)
            .await?;

        // Step 3: Update parent's children set
        // If this fails, forward/reverse keys are created
        // but parent's children map is not updated. Rollback is attempted below.
        let name_for_closure = name.clone();
        let inode_for_closure = inode;
        match self
            .update_parent_children(
                parent_inode,
                move |children| {
                    children.insert(name_for_closure.clone(), inode_for_closure);
                },
                10,
            )
            .await
        {
            Ok(_) => {
                info!(
                    "File created successfully: parent={}, name={}, inode={}",
                    parent_inode, name, inode
                );
                Ok(inode)
            }
            Err(e) => {
                // Compensation: Try to rollback the created entry
                error!(
                    "Failed to update parent children for file creation, attempting rollback: parent={}, name={}, inode={}, error={}",
                    parent_inode, name, inode, e
                );

                let rollback_keys = vec![forward_key.as_str(), reverse_key.as_str()];

                if let Err(rollback_err) =
                    self.delete_entry(&forward_key, &rollback_keys, inode).await
                {
                    error!(
                        "Failed to rollback file creation: inode={}, error={}. Manual cleanup may be required.",
                        inode, rollback_err
                    );
                }

                Err(MetaError::Internal(format!("Failed to create file: {}", e)))
            }
        }
    }

    /// Generate unique ID using local pool with batch allocation from Etcd
    /// Allocates 1000 IDs at a time to minimize etcd requests
    /// Supports multiple ID types (inode, slice, etc.) via different counter_key
    async fn generate_id(&self, counter_key: &str) -> Result<i64, MetaError> {
        let start = std::time::Instant::now();

        if let Some(id) = self.id_pools.try_alloc(counter_key).await {
            return Ok(id);
        }

        // Slow path: pool exhausted, need to allocate new batch from etcd
        info!(
            counter_key = counter_key,
            pool_hit = false,
            "Pool exhausted, allocating new batch from etcd"
        );

        let (allocated_id, next_start, pool_end) = self
            .atomic_update(
                counter_key,
                |current_id: i64| {
                    let next_etcd_id = current_id
                        .checked_add(BATCH_SIZE)
                        .ok_or_else(|| MetaError::Internal("ID counter overflow".to_string()))?;
                    Ok((next_etcd_id, (current_id, current_id + 1, next_etcd_id)))
                },
                || {
                    let next_etcd_id = FIRST_ALLOCATED_ID
                        .checked_add(BATCH_SIZE)
                        .ok_or_else(|| MetaError::Internal("ID counter overflow".to_string()))?;
                    Ok((
                        next_etcd_id,
                        (FIRST_ALLOCATED_ID, FIRST_ALLOCATED_ID + 1, next_etcd_id),
                    ))
                },
                10,
                &None,
            )
            .await?;

        self.id_pools
            .update(counter_key, next_start, pool_end)
            .await;

        let elapsed = start.elapsed();
        info!(
            counter_key = counter_key,
            allocated_id = allocated_id,
            batch_size = BATCH_SIZE,
            pool_remaining = next_start - pool_end,
            etcd_latency_ms = elapsed.as_millis() as u64,
            "ID batch allocated from etcd"
        );

        Ok(allocated_id)
    }

    /// Optimistic concurrency helper for etcd keys.
    ///
    /// `f` runs when the key exists and returns the new persisted value plus a payload `R` derived
    /// from the old state. `init` lazily provides the same tuple when the key is absent to avoid a
    /// separate create path. Both closures can error so domain-level failures propagate cleanly.
    /// Retries use exponential backoff on compare failures.
    async fn atomic_update<F, I, T, R>(
        &self,
        key: &str,
        f: F,
        init: I,
        max_retries: u64,
        options: &Option<PutOptions>,
    ) -> Result<R, MetaError>
    where
        F: Fn(T) -> Result<(T, R), MetaError>,
        I: Fn() -> Result<(T, R), MetaError>,
        T: Serialize + DeserializeOwned,
    {
        let client = self.client.clone();

        let f = || {
            // Cloning client is cheap.
            let mut client = client.clone();

            // Capture f by ref to avoid clone.
            let f = &f;
            let init = &init;

            async move {
                let resp = client
                    .get(key, None)
                    .await
                    .map_err(|e| MetaError::Config(format!("Failed to get key: {e}")))?;

                let (updated, ret, mod_revision) = match resp.kvs().first() {
                    Some(kv) => {
                        let current = serde_json::from_slice::<T>(kv.value())?;
                        let (value, r) = f(current)?;
                        (value, r, kv.mod_revision())
                    }
                    None => {
                        let (value, r) = init()?;
                        // When a key doesn't exist, there is no mod_revision. However, the version of a non-existent key
                        // is 0. So the following `compare` use `Compare::version`.
                        (value, r, 0)
                    }
                };
                let current = serde_json::to_string(&updated)?;

                let compare = if mod_revision == 0 {
                    Compare::version(key, CompareOp::Equal, 0)
                } else {
                    Compare::mod_revision(key, CompareOp::Equal, mod_revision)
                };
                let txn = Txn::new().when([compare]).and_then([TxnOp::put(
                    key,
                    current,
                    options.clone(),
                )]);

                match client.txn(txn).await {
                    Ok(txn_resp) if txn_resp.succeeded() => Ok(ret),
                    Ok(_) => Err(MetaError::ContinueRetry),
                    Err(e) => Err(MetaError::Internal(format!(
                        "Failed to execute transaction: {e}"
                    ))),
                }
            }
        };

        backoff(max_retries, f).await
    }

    /// Get a clone of the etcd client (for Watch Worker)
    pub fn get_client(&self) -> EtcdClient {
        self.client.clone()
    }

    /// Create entry with conflict check
    ///
    /// Atomically creates multiple key-value pairs only if the check key doesn't exist.
    /// This ensures no duplicate entries are created in concurrent scenarios.
    ///
    /// Uses `version == 0` check which correctly handles both cases:
    /// - Key never existed: version is 0
    /// - Key was deleted: version becomes 0 after deletion
    async fn create_entry(
        &self,
        check_key: &str,
        entries: &[(&str, &str)],
        parent: i64,
        name: &str,
    ) -> Result<(), MetaError> {
        let mut client = self.client.clone();
        // Use version instead of create_revision to handle deleted keys correctly
        // version == 0 means the key is currently not present (never existed or deleted)
        let mut txn = Txn::new().when([Compare::version(check_key, CompareOp::Equal, 0)]);
        let mut ops = Vec::new();
        for (key, value) in entries {
            ops.push(TxnOp::put(*key, *value, None));
        }
        txn = txn.and_then(ops);

        let resp = client
            .txn(txn)
            .await
            .map_err(|e| MetaError::Internal(format!("Create entry transaction failed: {}", e)))?;

        if resp.succeeded() {
            Ok(())
        } else {
            Err(MetaError::AlreadyExists {
                parent,
                name: name.to_string(),
            })
        }
    }

    /// Delete entry with existence check
    ///
    /// Atomically deletes multiple keys only if the check key exists.
    /// This ensures the entry exists before attempting deletion.
    ///
    /// Uses `version > 0` check to verify key currently exists.
    async fn delete_entry(
        &self,
        check_key: &str,
        keys: &[&str],
        ino: i64,
    ) -> Result<(), MetaError> {
        let mut client = self.client.clone();
        // Use version > 0 to check if key currently exists
        let mut txn = Txn::new().when([Compare::version(check_key, CompareOp::Greater, 0)]);
        let mut ops = Vec::new();
        for key in keys {
            ops.push(TxnOp::delete(*key, None));
        }
        txn = txn.and_then(ops);

        let resp = client
            .txn(txn)
            .await
            .map_err(|e| MetaError::Internal(format!("Delete entry transaction failed: {}", e)))?;

        if resp.succeeded() {
            Ok(())
        } else {
            Err(MetaError::NotFound(ino))
        }
    }

    /// Update parent directory children
    ///
    /// Uses optimistic concurrency control to safely update the children map
    /// in multi-client scenarios. Retries on conflicts up to max_retries.
    async fn update_parent_children(
        &self,
        parent_ino: i64,
        updater: impl Fn(&mut HashMap<String, i64>) + Send + Sync,
        max_retries: usize,
    ) -> Result<(), MetaError> {
        let key = Self::etcd_children_key(parent_ino);

        self.atomic_update(
            &key,
            |dir: EtcdDirChildren| {
                let mut children = dir.children;
                updater(&mut children);
                Ok((EtcdDirChildren::new(parent_ino, children), ()))
            },
            || {
                let mut children = HashMap::new();
                updater(&mut children);
                Ok((EtcdDirChildren::new(parent_ino, children), ()))
            },
            max_retries as u64,
            &None,
        )
        .await
        .map(|_| ())
        .map_err(|e| MetaError::Internal(format!("Update parent children failed: {e}")))
    }

    /// Check file is existing
    async fn file_is_existing(&self, inode: i64) -> Result<bool, MetaError> {
        let key = Self::etcd_reverse_key(inode);

        let entry_info: Option<EtcdEntryInfo> = self.etcd_get_json(&key).await?;
        match entry_info {
            Some(entry) => Ok(entry.is_file),
            None => Ok(false),
        }
    }

    async fn try_set_plock(
        &self,
        inode: i64,
        owner: i64,
        new_lock: &PlockRecord,
        lock_type: FileLockType,
        range: FileLockRange,
    ) -> Result<(), MetaError> {
        let key = Self::etcd_plock_key(inode);
        let sid = self
            .sid
            .get()
            .ok_or_else(|| MetaError::Internal("sid not set".to_string()))?;
        let lease = *self.get_lease()?;
        let put_options = Some(PutOptions::new().with_lease(lease));

        match lock_type {
            FileLockType::UnLock => {
                // Unlock file
                self.atomic_update(
                    &key,
                    |mut plocks: Vec<EtcdPlock>| {
                        // Find the lock record for this owner and sid
                        let pos = plocks
                            .iter()
                            .position(|p| p.sid == *sid && p.owner == owner);

                        if let Some(pos) = pos {
                            let plock = &mut plocks[pos];
                            let records: Vec<PlockRecord> = plock.records.clone();
                            if records.is_empty() {
                                // Remove this plock entry if no records
                                plocks.remove(pos);
                                return Ok((plocks, ()));
                            }

                            // Update locks with new unlock request
                            let new_records = PlockRecord::update_locks(records, *new_lock);

                            if new_records.is_empty() {
                                // Remove this plock entry if no records after update
                                plocks.remove(pos);
                                return Ok((plocks, ()));
                            }

                            // Update the records
                            plock.records = new_records;
                        }

                        Ok((plocks, ()))
                    },
                    || Ok((vec![], ())), // No existing locks, nothing to unlock
                    10,
                    &put_options,
                )
                .await
            }
            _ => {
                // Lock request (ReadLock or WriteLock)
                self.atomic_update(
                    &key,
                    |mut plocks: Vec<EtcdPlock>| {
                        // Build a hashmap of locks for easier lookup
                        let mut locks = HashMap::new();
                        for item in &plocks {
                            let key = (item.sid, item.owner);
                            locks.insert(key, item.records.clone());
                        }

                        let lkey = (*sid, owner);

                        // Check for conflicts with other owners/sessions
                        let mut conflict_found = false;
                        for ((sid, _owner), records_vec) in &locks {
                            if (*sid, owner) == lkey {
                                continue;
                            }

                            let ls: Vec<PlockRecord> = records_vec.clone(); // EtcdPlock already stores Vec<PlockRecord>
                            conflict_found = PlockRecord::check_conflict(&lock_type, &range, &ls);
                            if conflict_found {
                                break;
                            }
                        }

                        if conflict_found {
                            return Err(MetaError::LockConflict {
                                inode,
                                owner,
                                range,
                            });
                        }

                        // Get existing locks for this owner/session
                        let ls = locks.get(&lkey).cloned().unwrap_or_default();

                        // Update locks with new request
                        let ls = PlockRecord::update_locks(ls, *new_lock);

                        // Check if we need to update the record
                        if locks.get(&lkey).map(|r| r != &ls).unwrap_or(true) {
                            // Find existing plock entry and update it, or add new one
                            if let Some(plock) = plocks
                                .iter_mut()
                                .find(|p| p.sid == *sid && p.owner == owner)
                            {
                                plock.records = ls;
                            } else {
                                let new_plock = EtcdPlock {
                                    sid: *sid,
                                    owner,
                                    records: ls,
                                };
                                plocks.push(new_plock);
                            }
                        }

                        Ok((plocks, ()))
                    },
                    || {
                        // No existing locks, create new one
                        let ls = PlockRecord::update_locks(vec![], *new_lock);

                        let new_plock = EtcdPlock {
                            sid: *sid,
                            owner,
                            records: ls,
                        };

                        Ok((vec![new_plock], ()))
                    },
                    10,
                    &put_options,
                )
                .await
            }
        }
    }

    /// Update mtime and ctime for a directory inode
    async fn update_directory_timestamps(&self, ino: i64, now: i64) -> Result<(), MetaError> {
        let reverse_key = Self::etcd_reverse_key(ino);

        // Retry loop for optimistic locking using etcd's mod_revision
        let max_retries = 10;
        for retry in 0..max_retries {
            let mut client = self.client.clone();

            // Get current directory info with revision for CAS
            let get_resp = client.get(reverse_key.as_str(), None).await.map_err(|e| {
                MetaError::Internal(format!(
                    "Failed to get directory key {}: {}",
                    reverse_key, e
                ))
            })?;

            let kv = get_resp.kvs().first().ok_or(MetaError::NotFound(ino))?;
            let mod_revision = kv.mod_revision();

            let mut entry_info: EtcdEntryInfo =
                serde_json::from_slice(kv.value()).map_err(|e| {
                    MetaError::Internal(format!(
                        "Failed to deserialize directory entry info: {}",
                        e
                    ))
                })?;

            // Ensure this is a directory
            if entry_info.is_file {
                return Err(MetaError::Internal(format!(
                    "Cannot update directory timestamps for file {}",
                    ino
                )));
            }

            // Update timestamps
            entry_info.modify_time = now;
            entry_info.create_time = now; // ctime should also be updated

            // Attempt atomic update using mod_revision for precise CAS
            let txn = Txn::new()
                .when(vec![Compare::mod_revision(
                    reverse_key.as_bytes(),
                    CompareOp::Equal,
                    mod_revision,
                )])
                .and_then(vec![TxnOp::put(
                    reverse_key.as_bytes(),
                    serde_json::to_vec(&entry_info).map_err(|e| {
                        MetaError::Internal(format!(
                            "Failed to serialize directory entry info: {}",
                            e
                        ))
                    })?,
                    None,
                )]);

            match client.txn(txn).await {
                Ok(resp) if resp.succeeded() => {
                    return Ok(());
                }
                Ok(_resp) => {
                    // Transaction failed due to CAS conflict
                    if retry >= max_retries - 1 {
                        return Err(MetaError::Internal(format!(
                            "Failed to update directory timestamps for {} after {} retries",
                            ino, max_retries
                        )));
                    }
                    // Continue to next retry
                    tokio::time::sleep(std::time::Duration::from_millis(10 * (retry + 1) as u64))
                        .await;
                }
                Err(e) => {
                    return Err(MetaError::Internal(format!(
                        "Failed to update directory timestamps for {}: {}",
                        ino, e
                    )));
                }
            }
        }

        Ok(())
    }
    async fn shutdown_session_by_id(&self, session_id: Uuid) -> Result<(), MetaError> {
        let session_key = Self::etcd_session_key(Some(session_id));
        let session_info_key = Self::etcd_session_info_key(Some(session_id));
        let mut client = self.client.clone();
        let txn = Txn::new().and_then(vec![
            TxnOp::delete(session_key, None),
            TxnOp::delete(session_info_key, None),
        ]);
        client
            .txn(txn)
            .await
            .map_err(|err| MetaError::Internal(format!("Error shutting down session: {}", err)))?;
        Ok(())
    }
    fn set_sid(&self, session_id: Uuid) -> Result<(), MetaError> {
        self.sid
            .set(session_id)
            .map_err(|_| MetaError::Internal("sid has been set".to_string()))?;
        Ok(())
    }
    fn get_sid(&self) -> Result<&Uuid, MetaError> {
        self.sid
            .get()
            .ok_or_else(|| MetaError::Internal("sid has not been set".to_string()))
    }
    fn get_lease(&self) -> Result<&i64, MetaError> {
        self.lease
            .get()
            .ok_or_else(|| MetaError::Internal("lease has not been set".to_string()))
    }

    async fn life_cycle(token: CancellationToken, mut keeper: LeaseKeeper) {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(60));
        loop {
            select! {
                _ = interval.tick() => {
                    // refresh session
                    match keeper.keep_alive().await {
                        Ok(_) => {}
                        Err(err) => {
                            error!("Failed to refresh session: {}", err);
                        }
                    }

                }
                _ = token.cancelled() => {
                    break;
                }
            }
        }
    }
}

#[async_trait]
impl MetaStore for EtcdMetaStore {
    async fn stat(&self, ino: i64) -> Result<Option<FileAttr>, MetaError> {
        if let Ok(Some(file_meta)) = self.get_file_meta(ino).await {
            let permission = file_meta.permission();
            let kind = if file_meta.symlink_target.is_some() {
                FileType::Symlink
            } else {
                FileType::File
            };
            let size = if let Some(target) = &file_meta.symlink_target {
                target.len() as u64
            } else {
                file_meta.size as u64
            };
            return Ok(Some(FileAttr {
                ino: file_meta.inode,
                size,
                kind,
                mode: permission.mode,
                uid: permission.uid,
                gid: permission.gid,
                atime: file_meta.access_time,
                mtime: file_meta.modify_time,
                ctime: file_meta.create_time,
                nlink: file_meta.nlink as u32,
            }));
        }

        if let Ok(Some(access_meta)) = self.get_access_meta(ino).await {
            let permission = access_meta.permission();
            return Ok(Some(FileAttr {
                ino: access_meta.inode,
                size: 4096,
                kind: FileType::Dir,
                mode: permission.mode,
                uid: permission.uid,
                gid: permission.gid,
                atime: access_meta.access_time,
                mtime: access_meta.modify_time,
                ctime: access_meta.create_time,
                nlink: access_meta.nlink as u32,
            }));
        }

        Ok(None)
    }

    async fn lookup(&self, parent: i64, name: &str) -> Result<Option<i64>, MetaError> {
        let forward_key = Self::etcd_forward_key(parent, name);
        if let Some(entry) = self.etcd_get_json::<EtcdForwardEntry>(&forward_key).await? {
            Ok(Some(entry.inode))
        } else {
            Ok(None)
        }
    }

    async fn lookup_path(&self, path: &str) -> Result<Option<(i64, FileType)>, MetaError> {
        if path == "/" {
            return Ok(Some((1, FileType::Dir)));
        }

        let parts: Vec<&str> = path
            .trim_matches('/')
            .split('/')
            .filter(|p| !p.is_empty())
            .collect();
        let mut current_inode = 1i64;

        for (index, part) in parts.iter().enumerate() {
            let contents = self.get_content_meta(current_inode).await?;

            let found_entry = match contents {
                Some(entries) => entries.into_iter().find(|entry| entry.entry_name == *part),
                None => return Ok(None),
            };

            match found_entry {
                Some(entry) => match entry.entry_type {
                    EntryType::Directory => {
                        current_inode = entry.inode;
                    }
                    EntryType::File => {
                        if index == parts.len() - 1 {
                            return Ok(Some((entry.inode, FileType::File)));
                        } else {
                            return Ok(None);
                        }
                    }
                    EntryType::Symlink => {
                        if index == parts.len() - 1 {
                            return Ok(Some((entry.inode, FileType::Symlink)));
                        } else {
                            return Ok(None);
                        }
                    }
                },
                None => return Ok(None),
            }
        }

        Ok(Some((current_inode, FileType::Dir)))
    }

    async fn readdir(&self, ino: i64) -> Result<Vec<DirEntry>, MetaError> {
        let access_meta = self
            .get_access_meta(ino)
            .await?
            .ok_or(MetaError::NotFound(ino))?;

        let permission = access_meta.permission();
        if !permission.is_directory() {
            return Err(MetaError::NotDirectory(ino));
        }

        let contents = match self.get_content_meta(ino).await? {
            Some(contents) => contents,
            None => return Ok(Vec::new()),
        };

        let mut entries = Vec::new();
        for content in contents {
            let kind = match content.entry_type {
                EntryType::File => FileType::File,
                EntryType::Directory => FileType::Dir,
                EntryType::Symlink => FileType::Symlink,
            };
            entries.push(DirEntry {
                name: content.entry_name,
                ino: content.inode,
                kind,
            });
        }

        Ok(entries)
    }

    async fn mkdir(&self, parent: i64, name: String) -> Result<i64, MetaError> {
        self.create_directory(parent, name).await
    }

    async fn rmdir(&self, parent: i64, name: &str) -> Result<(), MetaError> {
        let forward_key = Self::etcd_forward_key(parent, name);
        let forward_entry: EtcdForwardEntry =
            match self.etcd_get_json::<EtcdForwardEntry>(&forward_key).await? {
                Some(fe) => fe,
                None => return Err(MetaError::NotFound(parent)),
            };

        let child_ino = forward_entry.inode;

        if forward_entry.is_file {
            return Err(MetaError::Internal("Not a directory".to_string()));
        }

        let children_key = Self::etcd_children_key(child_ino);
        if let Some(children) = self.etcd_get_json::<EtcdDirChildren>(&children_key).await?
            && !children.children.is_empty()
        {
            return Err(MetaError::DirectoryNotEmpty(child_ino));
        }

        info!(
            "Deleting directory with transaction: parent={}, name={}, inode={}",
            parent, name, child_ino
        );

        // Step 1: Delete the directory entries first (forward, reverse, children keys)
        // This ensures the directory is properly deleted before updating parent
        let reverse_key = Self::etcd_reverse_key(child_ino);
        let children_key = Self::etcd_children_key(child_ino);
        let delete_keys = vec![
            forward_key.as_str(),
            reverse_key.as_str(),
            children_key.as_str(),
        ];

        match self
            .delete_entry(&forward_key, &delete_keys, child_ino)
            .await
        {
            Ok(_) => {
                // Step 2: Directory deleted successfully, now update parent children map
                // If this fails, forward key is deleted but parent still references it
                // This is acceptable: lookup will fail (forward key gone), no dangling data
                let name_for_closure = name.to_string();

                match self
                    .update_parent_children(
                        parent,
                        move |children| {
                            children.remove(&name_for_closure);
                        },
                        10,
                    )
                    .await
                {
                    Ok(_) => {
                        info!(
                            "Directory deleted successfully: parent={}, name={}, inode={}",
                            parent, name, child_ino
                        );
                        Ok(())
                    }
                    Err(e) => {
                        // Parent update failed, but directory is deleted
                        // Forward key is deleted, so lookup will fail correctly
                        // Log warning but don't fail the operation
                        warn!(
                            "Rmdir succeeded but failed to update parent children map: parent={}, name={}, inode={}, error={}. Forward key deleted, lookup will fail correctly.",
                            parent, name, child_ino, e
                        );
                        // Return success since the directory is effectively deleted
                        // (forward key deleted = directory is unreachable)
                        Ok(())
                    }
                }
            }
            Err(e) => {
                // Deletion failed
                error!(
                    "Directory deletion failed: parent={}, name={}, inode={}, error={}",
                    parent, name, child_ino, e
                );
                Err(e)
            }
        }
    }

    async fn create_file(&self, parent: i64, name: String) -> Result<i64, MetaError> {
        self.create_file_internal(parent, name).await
    }

    async fn link(&self, ino: i64, parent: i64, name: &str) -> Result<FileAttr, MetaError> {
        if ino == 1 {
            return Err(MetaError::NotSupported(
                "cannot create hard links to the root inode".into(),
            ));
        }

        let parent_meta = self
            .get_access_meta(parent)
            .await?
            .ok_or(MetaError::ParentNotFound(parent))?;
        if !parent_meta.permission().is_directory() {
            return Err(MetaError::NotDirectory(parent));
        }

        if self.lookup(parent, name).await?.is_some() {
            return Err(MetaError::AlreadyExists {
                parent,
                name: name.to_string(),
            });
        }

        let reverse_key = Self::etcd_reverse_key(ino);
        let mut entry_info = self
            .etcd_get_json::<EtcdEntryInfo>(&reverse_key)
            .await?
            .ok_or(MetaError::NotFound(ino))?;

        if !entry_info.is_file {
            return Err(MetaError::NotSupported(
                "hard links are only supported for files and symlinks".into(),
            ));
        }
        if entry_info.deleted || entry_info.nlink == 0 {
            return Err(MetaError::NotFound(ino));
        }

        let old_nlink = entry_info.nlink;
        let now = Utc::now().timestamp_nanos_opt().unwrap_or(0);
        entry_info.nlink = entry_info.nlink.saturating_add(1);
        entry_info.modify_time = now;
        entry_info.create_time = now; // Update ctime when creating hard link
        entry_info.deleted = false;

        if old_nlink == 1 {
            // First hardlink: transition to LinkParent mode

            let old_parent = entry_info.parent_inode;
            let old_entry_name = entry_info.entry_name.clone();

            // Use link_parent instead of parent
            entry_info.parent_inode = 0;
            entry_info.entry_name = String::new();
            let link_parent_key = Self::etcd_link_parent_key(ino);
            let link_parents = vec![
                EtcdLinkParent {
                    parent_inode: old_parent,
                    entry_name: old_entry_name,
                },
                EtcdLinkParent {
                    parent_inode: parent,
                    entry_name: name.to_string(),
                },
            ];

            self.etcd_put_json(&link_parent_key, &link_parents, None)
                .await?;
        } else if old_nlink > 1 {
            // Already using LinkParent, just add new entry
            let link_parent_key = Self::etcd_link_parent_key(ino);

            self.atomic_update(
                &link_parent_key,
                |mut link_parents: Vec<EtcdLinkParent>| {
                    link_parents.push(EtcdLinkParent {
                        parent_inode: parent,
                        entry_name: name.to_string(),
                    });
                    Ok((link_parents, ()))
                },
                || {
                    Err(MetaError::Internal(format!(
                        "LinkParent key {} not found for inode {}",
                        link_parent_key, ino
                    )))
                },
                10,
                &None,
            )
            .await?;
        }

        let forward_key = Self::etcd_forward_key(parent, name);
        let entry_type = if entry_info.symlink_target.is_some() {
            EntryType::Symlink
        } else {
            EntryType::File
        };
        let forward_entry = EtcdForwardEntry {
            parent_inode: parent,
            name: name.to_string(),
            inode: ino,
            is_file: true,
            entry_type: Some(entry_type),
        };

        let updated_json = serde_json::to_string(&entry_info).map_err(|e| {
            MetaError::Internal(format!(
                "Failed to serialize entry info during link operation: {e}"
            ))
        })?;
        let forward_json = serde_json::to_string(&forward_entry).map_err(|e| {
            MetaError::Internal(format!(
                "Failed to serialize forward entry during link operation: {e}"
            ))
        })?;

        info!(
            "Creating hard link with atomic transaction: src_inode={}, parent={}, name={}",
            ino, parent, name
        );

        let mut client = self.client.clone();
        let txn = Txn::new()
            .when([
                Compare::version(forward_key.clone(), CompareOp::Equal, 0),
                Compare::version(reverse_key.clone(), CompareOp::Greater, 0),
            ])
            .and_then([
                TxnOp::put(forward_key.clone(), forward_json, None),
                TxnOp::put(reverse_key.clone(), updated_json, None),
            ]);

        let resp = client
            .txn(txn)
            .await
            .map_err(|e| MetaError::Internal(format!("Atomic link transaction failed: {e}")))?;

        if !resp.succeeded() {
            if self.lookup(parent, name).await?.is_some() {
                return Err(MetaError::AlreadyExists {
                    parent,
                    name: name.to_string(),
                });
            }
            if self
                .etcd_get_json::<EtcdEntryInfo>(&reverse_key)
                .await?
                .is_none()
            {
                return Err(MetaError::NotFound(ino));
            }
            return Err(MetaError::Internal(
                "Atomic link transaction failed unexpected compare".into(),
            ));
        }

        let name_for_closure = name.to_string();
        let ino_for_closure = ino;
        if let Err(e) = self
            .update_parent_children(
                parent,
                move |children| {
                    children.insert(name_for_closure.clone(), ino_for_closure);
                },
                10,
            )
            .await
        {
            warn!(
                "Link succeeded but failed to update parent children map: parent={}, name={}, inode={}, error={}. Forward key created, lookup remains consistent.",
                parent, name, ino, e
            );
        }

        self.stat(ino).await?.ok_or(MetaError::NotFound(ino))
    }

    async fn symlink(
        &self,
        parent: i64,
        name: &str,
        target: &str,
    ) -> Result<(i64, FileAttr), MetaError> {
        let parent_meta = self
            .get_access_meta(parent)
            .await?
            .ok_or(MetaError::ParentNotFound(parent))?;
        if !parent_meta.permission().is_directory() {
            return Err(MetaError::NotDirectory(parent));
        }

        if self.lookup(parent, name).await?.is_some() {
            return Err(MetaError::AlreadyExists {
                parent,
                name: name.to_string(),
            });
        }

        let inode = self.generate_id(INODE_ID_KEY).await?;
        let now = Utc::now().timestamp_nanos_opt().unwrap_or(0);
        let owner_uid = parent_meta.permission().uid;
        let owner_gid = parent_meta.permission().gid;
        let perm = Permission::new(0o120777, owner_uid, owner_gid);

        let entry_info = EtcdEntryInfo {
            is_file: true,
            size: Some(target.len() as i64),
            version: Some(0),
            permission: perm,
            access_time: now,
            modify_time: now,
            create_time: now,
            nlink: 1,
            parent_inode: parent,
            entry_name: name.to_string(),
            deleted: false,
            symlink_target: Some(target.to_string()),
        };

        let forward_key = Self::etcd_forward_key(parent, name);
        let forward_entry = EtcdForwardEntry {
            parent_inode: parent,
            name: name.to_string(),
            inode,
            is_file: true,
            entry_type: Some(EntryType::Symlink),
        };

        let reverse_key = Self::etcd_reverse_key(inode);
        let forward_json = serde_json::to_string(&forward_entry).map_err(|e| {
            MetaError::Internal(format!("Failed to serialize symlink forward entry: {e}"))
        })?;
        let reverse_json = serde_json::to_string(&entry_info).map_err(|e| {
            MetaError::Internal(format!("Failed to serialize symlink entry info: {e}"))
        })?;

        info!(
            "Creating symlink with transaction: parent={}, name={}, target={} -> inode={}",
            parent, name, target, inode
        );

        let operations = vec![
            (forward_key.as_str(), forward_json.as_str()),
            (reverse_key.as_str(), reverse_json.as_str()),
        ];

        self.create_entry(&forward_key, &operations, parent, name)
            .await?;

        let name_for_closure = name.to_string();
        let inode_for_closure = inode;
        if let Err(e) = self
            .update_parent_children(
                parent,
                move |children| {
                    children.insert(name_for_closure.clone(), inode_for_closure);
                },
                10,
            )
            .await
        {
            error!(
                "Symlink created but failed to update parent children map: parent={}, name={}, inode={}, error={}. Rolling back forward entry.",
                parent, name, inode, e
            );

            let rollback_keys = vec![forward_key.as_str(), reverse_key.as_str()];
            if let Err(rollback_err) = self.delete_entry(&forward_key, &rollback_keys, inode).await
            {
                error!(
                    "Failed to rollback symlink creation after parent update error: inode={}, error={}",
                    inode, rollback_err
                );
            }

            return Err(MetaError::Internal(format!(
                "Failed to update parent children map: {e}"
            )));
        }

        let attr = self.stat(inode).await?.ok_or(MetaError::NotFound(inode))?;
        Ok((inode, attr))
    }

    async fn read_symlink(&self, ino: i64) -> Result<String, MetaError> {
        let reverse_key = Self::etcd_reverse_key(ino);
        let entry_info = self
            .etcd_get_json::<EtcdEntryInfo>(&reverse_key)
            .await?
            .ok_or(MetaError::NotFound(ino))?;

        if entry_info.deleted {
            return Err(MetaError::NotFound(ino));
        }

        entry_info
            .symlink_target
            .ok_or_else(|| MetaError::NotSupported(format!("inode {ino} is not a symbolic link")))
    }

    async fn unlink(&self, parent: i64, name: &str) -> Result<(), MetaError> {
        let mut client = self.client.clone();

        let forward_key = Self::etcd_forward_key(parent, name);
        let forward_entry: EtcdForwardEntry =
            match self.etcd_get_json::<EtcdForwardEntry>(&forward_key).await? {
                Some(fe) => fe,
                None => return Err(MetaError::NotFound(parent)),
            };

        let file_ino = forward_entry.inode;

        if !forward_entry.is_file {
            return Err(MetaError::Internal("Is a directory".to_string()));
        }

        // Get current file metadata
        let reverse_key = Self::etcd_reverse_key(file_ino);
        let mut entry_info: EtcdEntryInfo =
            match self.etcd_get_json::<EtcdEntryInfo>(&reverse_key).await? {
                Some(info) => info,
                None => return Err(MetaError::NotFound(file_ino)),
            };

        let current_nlink = entry_info.nlink;
        let now = Utc::now().timestamp_nanos_opt().unwrap_or(0);

        if current_nlink > 1 {
            let link_parent_key = Self::etcd_link_parent_key(file_ino);

            // 2->1 transition: Need to restore parent before deleting LinkParent
            if current_nlink == 2 {
                // Get LinkParent entries
                let link_parents: Vec<EtcdLinkParent> = self
                    .etcd_get_json(&link_parent_key)
                    .await?
                    .ok_or_else(|| {
                        MetaError::Internal(format!(
                            "LinkParent key not found for inode {} with nlink=2. Data inconsistency detected!",
                            file_ino
                        ))
                    })?;

                // Find the remaining entry
                let remaining = link_parents
                    .iter()
                    .find(|lp| lp.parent_inode != parent || lp.entry_name != name)
                    .ok_or_else(|| {
                        MetaError::Internal(format!(
                            "No remaining LinkParent found for inode {} during 2->1 transition",
                            file_ino
                        ))
                    })?;

                // Restore parent and entry_name from remaining entry
                entry_info.parent_inode = remaining.parent_inode;
                entry_info.entry_name = remaining.entry_name.clone();

                // Delete all LinkParent entries
                client
                    .delete(link_parent_key.clone(), None)
                    .await
                    .map_err(|e| {
                        MetaError::Internal(format!(
                            "Failed to delete LinkParent for inode {}: {}",
                            file_ino, e
                        ))
                    })?;

                entry_info.nlink = 1;
                entry_info.deleted = false;
            } else {
                // n->n-1 transition (n > 2): Just remove the specific LinkParent entry
                self.atomic_update(
                    &link_parent_key,
                    |link_parents: Vec<EtcdLinkParent>| {
                        let updated_parents: Vec<EtcdLinkParent> = link_parents
                            .into_iter()
                            .filter(|lp| lp.parent_inode != parent || lp.entry_name != name)
                            .collect();
                        Ok((updated_parents, ()))
                    },
                    || {
                        Err(MetaError::Internal(format!(
                            "LinkParent key {} not found for inode {}",
                            link_parent_key, file_ino
                        )))
                    },
                    10,
                    &None,
                )
                .await?;

                entry_info.nlink = current_nlink - 1;
                entry_info.deleted = false;
            }
        } else {
            // 1->0 transition: Mark as deleted
            entry_info.deleted = true;
            entry_info.nlink = 0;
            entry_info.parent_inode = 0;
        }

        entry_info.modify_time = now;

        let updated_json = serde_json::to_string(&entry_info)
            .map_err(|e| MetaError::Internal(format!("Failed to serialize entry info: {}", e)))?;

        info!(
            "Unlinking file with atomic transaction: parent={}, name={}, inode={}",
            parent, name, file_ino
        );

        // Step 1: Perform atomic transaction first (delete forward key, update metadata)
        // This ensures the file is properly marked as deleted before updating parent
        let txn = Txn::new()
            .when([Compare::version(forward_key.clone(), CompareOp::Greater, 0)])
            .and_then([
                TxnOp::delete(forward_key.clone(), None),
                TxnOp::put(reverse_key.clone(), updated_json.clone(), None),
            ]);

        match client.txn(txn).await {
            Ok(resp) if resp.succeeded() => {
                // Step 2: Atomic transaction succeeded, now update parent children map
                // If this fails, forward key is deleted but parent still references it
                // This is acceptable: lookup will fail (forward key gone), no dangling data
                let name_for_closure = name.to_string();

                match self
                    .update_parent_children(
                        parent,
                        move |children| {
                            children.remove(&name_for_closure);
                        },
                        10,
                    )
                    .await
                {
                    Ok(_) => {
                        info!(
                            "File unlinked successfully: parent={}, name={}, inode={}",
                            parent, name, file_ino
                        );
                        Ok(())
                    }
                    Err(e) => {
                        // Parent update failed, but atomic transaction succeeded
                        // Forward key is deleted, so lookup will fail correctly
                        // Log warning but don't fail the operation
                        warn!(
                            "Unlink succeeded but failed to update parent children map: parent={}, name={}, inode={}, error={}. Forward key deleted, lookup will fail correctly.",
                            parent, name, file_ino, e
                        );
                        // Return success since the file is effectively unlinked
                        // (forward key deleted = file is unreachable)
                        Ok(())
                    }
                }
            }
            Ok(_) => {
                // Transaction failed (forward key not found)
                error!(
                    "Atomic unlink transaction failed (file not found): parent={}, name={}, inode={}",
                    parent, name, file_ino
                );
                Err(MetaError::NotFound(file_ino))
            }
            Err(e) => {
                // Transaction error
                error!(
                    "Atomic unlink transaction error: parent={}, name={}, inode={}, error={}",
                    parent, name, file_ino, e
                );
                Err(MetaError::Internal(format!(
                    "Atomic unlink transaction failed: {}",
                    e
                )))
            }
        }
    }

    async fn rename(
        &self,
        old_parent: i64,
        old_name: &str,
        new_parent: i64,
        new_name: String,
    ) -> Result<(), MetaError> {
        let old_forward_key = Self::etcd_forward_key(old_parent, old_name);
        let forward_entry = self
            .etcd_get_json::<EtcdForwardEntry>(&old_forward_key)
            .await?
            .ok_or(MetaError::NotFound(old_parent))?;

        let entry_ino = forward_entry.inode;
        let is_file = forward_entry.is_file;
        let entry_type = forward_entry.entry_type.clone();

        let reverse_key = Self::etcd_reverse_key(entry_ino);
        let mut entry_info = self
            .etcd_get_json::<EtcdEntryInfo>(&reverse_key)
            .await?
            .ok_or(MetaError::NotFound(entry_ino))?;

        // Prepare updated entry info
        entry_info.parent_inode = new_parent;
        entry_info.entry_name = new_name.clone();
        entry_info.modify_time = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0);
        let updated_reverse_json = serde_json::to_string(&entry_info)?;

        let new_forward_key = Self::etcd_forward_key(new_parent, &new_name);
        let new_forward_entry = EtcdForwardEntry {
            parent_inode: new_parent,
            name: new_name.clone(),
            inode: entry_ino,
            is_file,
            entry_type,
        };
        let new_forward_json = serde_json::to_string(&new_forward_entry)?;

        info!(
            "Renaming with atomic transaction: {} (parent={}) -> {} (parent={}), inode={}",
            old_name, old_parent, new_name, new_parent, entry_ino
        );

        // Atomic transaction: rename forward index AND update reverse index
        let mut client = self.client.clone();
        let txn = Txn::new()
            .when([
                Compare::create_revision(old_forward_key.clone(), CompareOp::NotEqual, 0),
                Compare::create_revision(new_forward_key.clone(), CompareOp::Equal, 0),
            ])
            .and_then([
                TxnOp::put(new_forward_key.clone(), new_forward_json, None),
                TxnOp::delete(old_forward_key.clone(), None),
                TxnOp::put(reverse_key.clone(), updated_reverse_json, None),
            ]);

        let resp = client
            .txn(txn)
            .await
            .map_err(|e| MetaError::Internal(format!("Atomic rename transaction failed: {}", e)))?;

        if !resp.succeeded() {
            // Check which condition failed
            let source_resp = client
                .get(old_forward_key, None)
                .await
                .map_err(|e| MetaError::Internal(format!("Failed to check source: {}", e)))?;

            if source_resp.kvs().is_empty() {
                return Err(MetaError::NotFound(entry_ino));
            } else {
                return Err(MetaError::AlreadyExists {
                    parent: new_parent,
                    name: new_name,
                });
            }
        }

        // Atomic transaction succeeded, now update parent children maps
        // If parent updates fail, forward/reverse keys already reflect the new state
        // This is acceptable: lookup will work correctly with new forward key

        // Update parent children maps
        // Note: Parent children map updates are NOT atomic with the forward/reverse key updates
        // This is a known limitation. If these updates fail:
        // - Forward key points to new location (correct)
        // - Parent children map may be stale (but lookup still works via forward key)
        // - Consider this acceptable as forward key is the source of truth

        if old_parent != new_parent {
            // Different parents: remove from old, add to new
            let old_name_for_closure = old_name.to_string();
            match self
                .update_parent_children(
                    old_parent,
                    move |children| {
                        children.remove(&old_name_for_closure);
                    },
                    10,
                )
                .await
            {
                Ok(_) => {}
                Err(e) => {
                    warn!(
                        "Rename succeeded but failed to update old parent children map: old_parent={}, name={}, inode={}, error={}. Forward key updated, lookup will work correctly.",
                        old_parent, old_name, entry_ino, e
                    );
                    // Don't fail the operation - forward key is already updated
                }
            }

            let new_name_for_closure = new_name.clone();
            let entry_ino_for_closure = entry_ino;
            match self
                .update_parent_children(
                    new_parent,
                    move |children| {
                        children.insert(new_name_for_closure.clone(), entry_ino_for_closure);
                    },
                    10,
                )
                .await
            {
                Ok(_) => {}
                Err(e) => {
                    warn!(
                        "Rename succeeded but failed to update new parent children map: new_parent={}, name={}, inode={}, error={}. Forward key updated, lookup will work correctly.",
                        new_parent, new_name, entry_ino, e
                    );
                    // Don't fail the operation - forward key is already updated
                }
            }
        } else if old_name != new_name {
            // Same parent, different name: single atomic update
            let old_name_for_closure = old_name.to_string();
            let new_name_for_closure = new_name.clone();
            let entry_ino_for_closure = entry_ino;
            match self
                .update_parent_children(
                    new_parent,
                    move |children| {
                        children.remove(&old_name_for_closure);
                        children.insert(new_name_for_closure.clone(), entry_ino_for_closure);
                    },
                    10,
                )
                .await
            {
                Ok(_) => {}
                Err(e) => {
                    warn!(
                        "Rename succeeded but failed to update parent children map: parent={}, old_name={}, new_name={}, inode={}, error={}. Forward key updated, lookup will work correctly.",
                        new_parent, old_name, new_name, entry_ino, e
                    );
                    // Don't fail the operation - forward key is already updated
                }
            }
        }

        // Update parent directory timestamps
        let now = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0);

        // Update old parent directory timestamps
        if let Err(e) = self.update_directory_timestamps(old_parent, now).await {
            warn!(
                "Rename succeeded but failed to update old parent directory timestamps: old_parent={}, error={}",
                old_parent, e
            );
        }

        // Update new parent directory timestamps
        if let Err(e) = self.update_directory_timestamps(new_parent, now).await {
            warn!(
                "Rename succeeded but failed to update new parent directory timestamps: new_parent={}, error={}",
                new_parent, e
            );
        }

        info!(
            "Rename completed successfully: {} -> {}, inode={}",
            old_name, new_name, entry_ino
        );

        Ok(())
    }

    async fn set_file_size(&self, ino: i64, size: u64) -> Result<(), MetaError> {
        let reverse_key = Self::etcd_reverse_key(ino);
        self.atomic_update(
            &reverse_key,
            |mut entry_info: EtcdEntryInfo| {
                if !entry_info.is_file {
                    return Err(MetaError::Internal(
                        "Cannot set size for directory".to_string(),
                    ));
                }

                entry_info.size = Some(size as i64);
                entry_info.modify_time = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0);
                Ok((entry_info, ()))
            },
            || Err(MetaError::NotFound(ino)),
            10,
            &None,
        )
        .await
        .map(|_| ())
    }

    async fn get_parent(&self, ino: i64) -> Result<Option<i64>, MetaError> {
        if ino == 1 {
            return Ok(None);
        }

        let reverse_key = Self::etcd_reverse_key(ino);
        if let Some(entry_info) = self.etcd_get_json::<EtcdEntryInfo>(&reverse_key).await? {
            Ok(Some(entry_info.parent_inode))
        } else {
            Ok(None)
        }
    }

    async fn get_name(&self, ino: i64) -> Result<Option<String>, MetaError> {
        if ino == 1 {
            return Ok(Some("/".to_string()));
        }

        let reverse_key = Self::etcd_reverse_key(ino);
        if let Some(entry_info) = self.etcd_get_json::<EtcdEntryInfo>(&reverse_key).await? {
            Ok(Some(entry_info.entry_name))
        } else {
            Ok(None)
        }
    }

    async fn get_path(&self, ino: i64) -> Result<Option<String>, MetaError> {
        if ino == 1 {
            return Ok(Some("/".to_string()));
        }

        let mut path_parts = Vec::new();
        let mut current_ino = ino;

        loop {
            let reverse_key = Self::etcd_reverse_key(current_ino);

            let entry_info = match self.etcd_get_json::<EtcdEntryInfo>(&reverse_key).await? {
                Some(info) => info,
                None => return Ok(None),
            };

            path_parts.push(entry_info.entry_name);

            let parent = entry_info.parent_inode;
            if parent == 1 {
                break;
            }

            current_ino = parent;
        }

        path_parts.reverse();
        let path = format!("/{}", path_parts.join("/"));
        Ok(Some(path))
    }

    fn root_ino(&self) -> i64 {
        1
    }

    async fn initialize(&self) -> Result<(), MetaError> {
        Ok(())
    }

    async fn get_deleted_files(&self) -> Result<Vec<i64>, MetaError> {
        let mut client = self.client.clone();

        // Get all keys with reverse index prefix "r:"
        let resp = client
            .get(
                "r:".to_string(),
                Some(etcd_client::GetOptions::new().with_prefix()),
            )
            .await
            .map_err(|e| MetaError::Internal(format!("Failed to scan for deleted files: {}", e)))?;

        let mut deleted_files = Vec::new();

        for kv in resp.kvs() {
            if let Ok(entry_info) = serde_json::from_slice::<EtcdEntryInfo>(kv.value()) {
                // Only include files that are marked as deleted
                if entry_info.is_file && entry_info.deleted {
                    // Extract inode from key (format: "r:{inode}")
                    let key_str = String::from_utf8_lossy(kv.key());
                    if let Some(inode_str) = key_str.strip_prefix("r:")
                        && let Ok(inode) = inode_str.parse::<i64>()
                    {
                        deleted_files.push(inode);
                    }
                }
            }
        }

        Ok(deleted_files)
    }

    async fn remove_file_metadata(&self, ino: i64) -> Result<(), MetaError> {
        let mut client = self.client.clone();

        let reverse_key = Self::etcd_reverse_key(ino);

        // Check if the file exists and is marked as deleted
        let entry_info: EtcdEntryInfo =
            match self.etcd_get_json::<EtcdEntryInfo>(&reverse_key).await? {
                Some(info) => info,
                None => return Err(MetaError::NotFound(ino)),
            };

        if !entry_info.is_file {
            return Err(MetaError::Internal(
                "Cannot remove directory metadata with remove_file_metadata".to_string(),
            ));
        }

        if !entry_info.deleted {
            return Err(MetaError::Internal(
                "File is not marked as deleted".to_string(),
            ));
        }

        // Delete the reverse index entry (file metadata)
        client
            .delete(reverse_key, None)
            .await
            .map_err(|e| MetaError::Internal(format!("Failed to remove file metadata: {}", e)))?;

        Ok(())
    }

    async fn get_slices(&self, chunk_id: u64) -> Result<Vec<SliceDesc>, MetaError> {
        let key = key_for_slice(chunk_id);
        self.etcd_get_json(&key)
            .await
            .map(|e| e.unwrap_or_default())
    }

    async fn append_slice(&self, chunk_id: u64, slice: SliceDesc) -> Result<(), MetaError> {
        let key = key_for_slice(chunk_id);

        // Note that `slice` is `Copy`.
        self.atomic_update(
            &key,
            |mut source: Vec<SliceDesc>| {
                source.push(slice);
                Ok((source, ()))
            },
            || Ok((vec![slice], ())),
            10,
            &None,
        )
        .await
        .map(|_| ())
    }

    async fn next_id(&self, key: &str) -> Result<i64, MetaError> {
        self.generate_id(key).await
    }

    // ---------- Session lifecycle implementation ----------

    async fn start_session(
        &self,
        session_info: SessionInfo,
        token: CancellationToken,
    ) -> Result<Session, MetaError> {
        let session_id = Uuid::now_v7();
        let session_key = Self::etcd_session_key(Some(session_id));
        let session_info_key = Self::etcd_session_info_key(Some(session_id));
        let expire = (Utc::now() + Duration::minutes(5)).timestamp_millis();
        let session = Session {
            session_id,
            session_info: session_info.clone(),
            expire,
        };

        let mut conn = self.client.clone();
        let lease = conn
            .lease_grant(60 * 5, None)
            .await
            .map_err(|e| MetaError::Internal(format!("Failed to grant lease: {e}")))?;
        let options = PutOptions::new().with_lease(lease.id());

        self.etcd_put_json(session_key, &expire, Some(options.clone()))
            .await?;
        self.etcd_put_json(session_info_key, &session_info, Some(options.clone()))
            .await?;
        let (keeper, _) = conn
            .lease_keep_alive(lease.id())
            .await
            .map_err(|e| MetaError::Internal(format!("Failed to create lease keeper: {e}")))?;

        tokio::spawn(Self::life_cycle(token, keeper));

        self.set_sid(session_id)?;
        self.lease
            .set(lease.id())
            .map_err(|_| MetaError::Internal("Failed to set lease".to_string()))?;
        Ok(session)
    }

    async fn shutdown_session(&self) -> Result<(), MetaError> {
        let session_id = *self.get_sid()?;
        self.shutdown_session_by_id(session_id).await?;
        Ok(())
    }

    // Etcd cleanup is performed by the lease keeper
    async fn cleanup_sessions(&self) -> Result<(), MetaError> {
        return Ok(());
    }
    async fn get_global_lock(&self, lock_name: LockName) -> bool {
        let result = self
            .atomic_update::<_, _, i64, bool>(
                &lock_name.to_string(),
                |old| {
                    let now = Utc::now().timestamp_millis();
                    if now > old + Duration::seconds(7).num_milliseconds() {
                        Ok((now, true))
                    } else {
                        Ok((old, false))
                    }
                },
                || {
                    let now = Utc::now().timestamp_millis();
                    Ok((now, true))
                },
                3,
                &None,
            )
            .await;

        match result {
            Ok(flag) => flag,
            Err(err) => {
                error!("Error getting lock: {}", err);
                false
            }
        }
    }

    async fn set_attr(
        &self,
        ino: i64,
        req: &SetAttrRequest,
        flags: SetAttrFlags,
    ) -> Result<FileAttr, MetaError> {
        let reverse_key = Self::etcd_reverse_key(ino);
        let now = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0);

        // Retry loop for optimistic locking using etcd's mod_revision
        let max_retries = 10;
        for retry in 0..max_retries {
            let mut client = self.client.clone();

            // Get current entry info with revision for CAS
            let get_resp = client.get(reverse_key.as_str(), None).await.map_err(|e| {
                MetaError::Internal(format!("Failed to get key {}: {}", reverse_key, e))
            })?;

            let kv = get_resp.kvs().first().ok_or(MetaError::NotFound(ino))?;
            let mod_revision = kv.mod_revision();

            let mut entry_info: EtcdEntryInfo =
                serde_json::from_slice(kv.value()).map_err(|e| {
                    MetaError::Internal(format!("Failed to deserialize entry info: {}", e))
                })?;

            let mut ctime_update = false;

            // Apply permission changes
            if let Some(mode) = req.mode {
                entry_info.permission.chmod(mode);
                ctime_update = true;
            }

            if let Some(uid) = req.uid {
                let gid = req.gid.unwrap_or(entry_info.permission.gid);
                entry_info.permission.chown(uid, gid);
                ctime_update = true;
            }

            if req.uid.is_none()
                && let Some(gid) = req.gid
            {
                entry_info.permission.chown(entry_info.permission.uid, gid);
                ctime_update = true;
            }

            // Clear suid/sgid flags
            if flags.contains(SetAttrFlags::CLEAR_SUID) {
                entry_info.permission.mode &= !0o4000;
                ctime_update = true;
            }
            if flags.contains(SetAttrFlags::CLEAR_SGID) {
                entry_info.permission.mode &= !0o2000;
                ctime_update = true;
            }

            // Apply size changes (files only)
            if entry_info.is_file
                && let Some(size_req) = req.size
            {
                let new_size = size_req as i64;
                if entry_info.size != Some(new_size) {
                    entry_info.size = Some(new_size);
                    entry_info.modify_time = now;
                }
                ctime_update = true;
            }

            // Apply time changes
            if flags.contains(SetAttrFlags::SET_ATIME_NOW) {
                entry_info.access_time = now;
                ctime_update = true;
            } else if let Some(atime) = req.atime {
                entry_info.access_time = atime;
                ctime_update = true;
            }

            if flags.contains(SetAttrFlags::SET_MTIME_NOW) {
                entry_info.modify_time = now;
                ctime_update = true;
            } else if let Some(mtime) = req.mtime {
                entry_info.modify_time = mtime;
                ctime_update = true;
            }

            if let Some(ctime) = req.ctime {
                entry_info.create_time = ctime;
            } else if ctime_update {
                entry_info.create_time = now;
            }

            // Attempt atomic update using mod_revision for precise CAS
            let txn = Txn::new()
                .when(vec![Compare::mod_revision(
                    reverse_key.as_bytes(),
                    CompareOp::Equal,
                    mod_revision,
                )])
                .and_then(vec![TxnOp::put(
                    reverse_key.as_bytes(),
                    serde_json::to_vec(&entry_info).map_err(|e| {
                        MetaError::Internal(format!("Failed to serialize entry info: {}", e))
                    })?,
                    None,
                )]);

            match client.txn(txn).await {
                Ok(resp) if resp.succeeded() => {
                    // Success - convert to FileAttr and return
                    let kind = if entry_info.symlink_target.is_some() {
                        FileType::Symlink
                    } else if entry_info.is_file {
                        FileType::File
                    } else {
                        FileType::Dir
                    };

                    let size = if let Some(target) = &entry_info.symlink_target {
                        target.len() as u64
                    } else if entry_info.is_file {
                        entry_info.size.unwrap_or(0).max(0) as u64
                    } else {
                        4096
                    };

                    return Ok(FileAttr {
                        ino,
                        size,
                        kind,
                        mode: entry_info.permission.mode,
                        uid: entry_info.permission.uid,
                        gid: entry_info.permission.gid,
                        atime: entry_info.access_time,
                        mtime: entry_info.modify_time,
                        ctime: entry_info.create_time,
                        nlink: entry_info.nlink,
                    });
                }
                Ok(_) => {
                    // Transaction failed (CAS conflict), retry
                    if retry < max_retries - 1 {
                        warn!(
                            "CAS conflict updating attributes for inode {} (retry {}/{})",
                            ino,
                            retry + 1,
                            max_retries
                        );
                        // Exponential backoff
                        tokio::time::sleep(tokio::time::Duration::from_millis(5 * (1 << retry)))
                            .await;
                        continue;
                    }
                }
                Err(e) => {
                    if retry < max_retries - 1 {
                        warn!(
                            "Failed to update attributes for inode {} (retry {}/{}): {}",
                            ino,
                            retry + 1,
                            max_retries,
                            e
                        );
                        tokio::time::sleep(tokio::time::Duration::from_millis(5 * (1 << retry)))
                            .await;
                        continue;
                    } else {
                        error!(
                            "Failed to update attributes for inode {} after {} retries: {}",
                            ino, max_retries, e
                        );
                        return Err(MetaError::Internal(format!(
                            "Failed to update attributes: {}",
                            e
                        )));
                    }
                }
            }
        }

        Err(MetaError::Internal(format!(
            "Failed to update attributes for inode {} after {} retries (CAS conflicts)",
            ino, max_retries
        )))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    // returns the current lock owner for a range on a file.
    async fn get_plock(
        &self,
        inode: i64,
        query: &FileLockQuery,
    ) -> Result<FileLockInfo, MetaError> {
        let key = Self::etcd_plock_key(inode);
        let sid = self
            .sid
            .get()
            .ok_or_else(|| MetaError::Internal("sid not set".to_string()))?;

        let plocks: Vec<EtcdPlock> = self.etcd_get_json(&key).await?.unwrap_or_default();

        for plock in plocks {
            let locks = &plock.records;
            if let Some(v) = PlockRecord::get_plock(locks, query, sid, &plock.sid) {
                return Ok(v);
            }
        }

        Ok(FileLockInfo {
            lock_type: FileLockType::UnLock,
            range: FileLockRange { start: 0, end: 0 },
            pid: 0,
        })
    }

    // sets a file range lock on given file.
    async fn set_plock(
        &self,
        inode: i64,
        owner: i64,
        block: bool,
        lock_type: FileLockType,
        range: FileLockRange,
        pid: u32,
    ) -> Result<(), MetaError> {
        let new_lock = PlockRecord::new(lock_type, pid, range.start, range.end);

        loop {
            let result = self
                .try_set_plock(inode, owner, &new_lock, lock_type, range)
                .await;

            match result {
                Ok(()) => return Ok(()),
                Err(MetaError::LockConflict { .. }) if block => {
                    if lock_type == FileLockType::Write {
                        tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
                    } else {
                        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
                    }
                    continue;
                }
                Err(e) => return Err(e),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::meta::MetaStore;
    use crate::meta::config::Config;
    use crate::meta::config::{CacheConfig, ClientOptions, DatabaseConfig, DatabaseType};
    use crate::meta::file_lock::{FileLockQuery, FileLockRange, FileLockType};
    use crate::meta::store::MetaError;
    use crate::meta::stores::EtcdMetaStore;
    use tokio::time;
    use uuid::Uuid;

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

    #[tokio::test]
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

    #[tokio::test]
    async fn test_multiple_read_locks() {
        // Create session manager with 2 sessions
        let session_mgr = TestSessionManager::new(2).await;

        let owner1: i64 = 1001;
        let owner2: i64 = 1002;

        // Create a file first using the first session
        let store1 = session_mgr.get_store(0);
        let parent = store1.root_ino();
        let file_ino = store1
            .create_file(parent, "test_multiple_read_locks_file.txt".to_string())
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

    #[tokio::test]
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

    #[tokio::test]
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

    #[tokio::test]
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
                owner1 as i64,
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
                owner2 as i64,
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

    #[tokio::test]
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
                    owner2 as i64,
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
                    owner3 as i64,
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

    #[tokio::test]
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
}
