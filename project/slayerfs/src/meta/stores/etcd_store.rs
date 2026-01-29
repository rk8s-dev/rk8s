//! Etcd-based metadata store implementation

use super::{apply_truncate_plan, trim_slices_in_place};
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
use crate::vfs::chunk_id_for;
use crate::vfs::fs::FileType;
use async_trait::async_trait;
use chrono::{Duration, Utc};
use etcd_client::{
    Client as EtcdClient, Compare, CompareOp, LeaseKeeper, PutOptions, Txn, TxnOp, TxnOpResponse,
};

use std::collections::HashMap;
use std::path::Path;
use std::sync::OnceLock;
use tokio::select;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};
use uuid::Uuid;

const BATCH_SIZE: i64 = 1000;
const FIRST_ALLOCATED_ID: i64 = 2;

pub struct EtcdMetaStore {
    client: EtcdClient,
    _config: Config,
    id_pools: IdPool,
    sid: OnceLock<Uuid>,
    lease: OnceLock<i64>,
}

#[allow(dead_code)]
impl EtcdMetaStore {
    /// Etcd helper method: generate forward index key (parent_inode, name)
    const KEY_PREFIX: &'static str = "slayerfs/";

    fn key(key: impl AsRef<str>) -> String {
        format!("{}{}", Self::KEY_PREFIX, key.as_ref())
    }

    fn etcd_forward_key(parent_inode: i64, name: &str) -> String {
        Self::key(format!("f:{}:{}", parent_inode, name))
    }

    fn etcd_reverse_key(ino: i64) -> String {
        Self::key(format!("r:{}", ino))
    }

    fn etcd_children_key(inode: i64) -> String {
        Self::key(format!("c:{}", inode))
    }

    fn etcd_session_key(session_id: Option<Uuid>) -> String {
        match session_id {
            Some(id) => Self::key(format!("session:{}", id)),
            None => Self::key("session:"),
        }
    }

    fn etcd_session_info_key(session_id: Option<Uuid>) -> String {
        match session_id {
            Some(id) => Self::key(format!("session_info:{}", id)),
            None => Self::key("session_info:"),
        }
    }

    fn get_session_id_from_session_key(session_key: &str) -> Option<Uuid> {
        session_key
            .strip_prefix(Self::KEY_PREFIX)
            .and_then(|key| key.strip_prefix("session:"))
            .and_then(|s| Uuid::parse_str(s).ok())
    }

    fn etcd_plock_key(inode: i64) -> String {
        Self::key(format!("p:{inode}"))
    }

    fn etcd_link_parent_key(inode: i64) -> String {
        Self::key(format!("l:{}", inode))
    }

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

    async fn etcd_get<T>(&self, key: &str) -> Result<Option<T>, MetaError>
    where
        T: serde::de::DeserializeOwned,
    {
        let mut client = self.client.clone();
        match client.get(key.to_string(), None).await {
            Ok(resp) => {
                if let Some(kv) = resp.kvs().first() {
                    let obj: T = serde_json::from_slice(kv.value()).map_err(MetaError::from)?;
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

    async fn etcd_put<T>(
        &self,
        key: impl AsRef<str>,
        obj: &T,
        options: Option<PutOptions>,
    ) -> Result<(), MetaError>
    where
        T: serde::Serialize,
    {
        let mut client = self.client.clone();

        let bytes = serde_json::to_vec(obj).map_err(MetaError::from)?;
        let key = key.as_ref();

        client
            .put(key, bytes, options)
            .await
            .map(|_| ())
            .map_err(|e| MetaError::Internal(format!("Failed to put key {key}: {e}")))
    }

    async fn prune_slices_for_truncate(
        &self,
        ino: i64,
        new_size: u64,
        old_size: u64,
        chunk_size: u64,
    ) -> Result<(), MetaError> {
        apply_truncate_plan(
            new_size,
            old_size,
            chunk_size,
            |cutoff_chunk, cutoff_offset| async move {
                let chunk_id = chunk_id_for(ino, cutoff_chunk);
                let key = key_for_slice(chunk_id);
                let mut slices: Vec<SliceDesc> = self.etcd_get(&key).await?.unwrap_or_default();
                trim_slices_in_place(&mut slices, cutoff_offset);
                if slices.is_empty() {
                    let mut client = self.client.clone();
                    client.delete(key.as_str(), None).await.map_err(|e| {
                        MetaError::Internal(format!("Failed to delete key {key}: {e}"))
                    })?;
                } else {
                    self.etcd_put(&key, &slices, None).await?;
                }
                Ok(())
            },
            |start, end| async move {
                for idx in start..end {
                    let chunk_id = chunk_id_for(ino, idx);
                    let key = key_for_slice(chunk_id);
                    let mut client = self.client.clone();
                    client.delete(key.as_str(), None).await.map_err(|e| {
                        MetaError::Internal(format!("Failed to delete key {key}: {e}"))
                    })?;
                }
                Ok(())
            },
        )
        .await
    }

    async fn etcd_get_lenient<T>(&self, key: &str) -> Result<Option<T>, MetaError>
    where
        T: serde::de::DeserializeOwned,
    {
        match self.etcd_get::<T>(key).await {
            Ok(v) => Ok(v),
            Err(e) => {
                error!("Etcd get failed for {}: {}", key, e);
                Ok(None)
            }
        }
    }

    async fn init_root_directory(&self) -> Result<(), MetaError> {
        let now = Utc::now().timestamp_nanos_opt().unwrap_or(0);

        let children_key = Self::etcd_children_key(1);
        let root_children = EtcdDirChildren::new(1, HashMap::new());
        let children_bytes = serde_json::to_vec(&root_children).map_err(MetaError::from)?;

        let reverse_key = Self::etcd_reverse_key(1);
        let root_entry = EtcdEntryInfo {
            is_file: false,
            size: None,
            version: None,
            permission: Permission::new(0o40755, 0, 0),
            access_time: now,
            modify_time: now,
            create_time: now,
            nlink: 2,
            parent_inode: 1,
            entry_name: "/".to_string(),
            deleted: false,
            symlink_target: None,
        };
        let reverse_bytes = serde_json::to_vec(&root_entry).map_err(MetaError::from)?;

        let mut client = self.client.clone();

        let txn = Txn::new()
            .when([Compare::version(children_key.clone(), CompareOp::Equal, 0)])
            .and_then([
                TxnOp::put(children_key.clone(), children_bytes, None),
                TxnOp::put(reverse_key, reverse_bytes, None),
            ]);

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
        if let Ok(Some(entry_info)) = self.etcd_get_lenient::<EtcdEntryInfo>(&reverse_key).await
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
            .etcd_get_lenient::<EtcdDirChildren>(&children_key)
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
        let forward_prefix = Self::key(format!("f:{}:", parent_inode));

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
        if let Ok(Some(entry_info)) = self.etcd_get_lenient::<EtcdEntryInfo>(&reverse_key).await
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
        let reverse_key = Self::etcd_reverse_key(inode);
        let children_key = Self::etcd_children_key(inode);

        let forward_entry = EtcdForwardEntry {
            parent_inode,
            name: name.clone(),
            inode,
            is_file: false,
            entry_type: Some(EntryType::Directory),
        };
        let children = EtcdDirChildren::new(inode, HashMap::new());

        let forward_value = serde_json::to_vec(&forward_entry).map_err(MetaError::from)?;
        let reverse_value = serde_json::to_vec(&entry_info).map_err(MetaError::from)?;
        let children_value = serde_json::to_vec(&children).map_err(MetaError::from)?;

        // Step 2: Atomic transaction - create all keys only if forward key doesn't exist
        info!(
            "Creating directory with transaction: parent={}, name={}, inode={}",
            parent_inode, name, inode
        );

        let operations = vec![
            (forward_key.as_str(), forward_value.as_slice()),
            (reverse_key.as_str(), reverse_value.as_slice()),
            (children_key.as_str(), children_value.as_slice()),
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
        let reverse_key = Self::etcd_reverse_key(inode);

        let forward_entry = EtcdForwardEntry {
            parent_inode,
            name: name.clone(),
            inode,
            is_file: true,
            entry_type: Some(EntryType::File),
        };
        let forward_value = serde_json::to_vec(&forward_entry).map_err(MetaError::from)?;
        let reverse_value = serde_json::to_vec(&entry_info).map_err(MetaError::from)?;


        // Step 2: Atomic transaction - create keys only if forward key doesn't exist
        info!(
            "Creating file with transaction: parent={}, name={}, inode={}",
            parent_inode, name, inode
        );

        let operations = vec![
            (forward_key.as_str(), forward_value.as_slice()),
            (reverse_key.as_str(), reverse_value.as_slice()),
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

        if let Some(id) = self.id_pools.try_alloc(counter_key) {
            return Ok(id);
        }

        // Slow path: pool exhausted, need to allocate new batch from etcd
        info!(
            counter_key = counter_key,
            pool_hit = false,
            "Pool exhausted, allocating new batch from etcd"
        );

        let (allocated_id, next_start, pool_end) = self
            .update_with_optimistic_concurrency(
                counter_key,
                None,
                10,
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
            )
            .await?;

        self.id_pools.update(counter_key, next_start, pool_end);

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

    async fn update_with_optimistic_concurrency<F, I, T, R>(
        &self,
        key: &str,
        options: Option<PutOptions>,
        max_retries: u32,
        f: F,
        init: I,
    ) -> Result<R, MetaError>
    where
        F: Fn(T) -> Result<(T, R), MetaError> + Send + Sync,
        I: Fn() -> Result<(T, R), MetaError>,
        T: serde::Serialize + serde::de::DeserializeOwned,
     {
         let client = self.client.clone();

         let f = || {
             // Cloning client is cheap.
             let mut client = client.clone();

             // Capture f by ref to avoid clone.
             let f = &f;
             let init = &init;
             let options_ref = &options;

             async move {
                 let resp = client
                     .get(key, None)
                     .await
                     .map_err(|e| MetaError::Config(format!("Failed to get key: {e}")))?;

                 let (updated, ret, mod_revision) = match resp.kvs().first() {
                     Some(kv) => {
                         let current: T = serde_json::from_slice(kv.value()).map_err(MetaError::from)?;
                         let (value, r) = f(current)?;
                         (value, r, kv.mod_revision())
                     }
                     None => {
                         let (value, r) = init()?;
                         (value, r, 0)
                     }
                 };

                 let current = serde_json::to_vec(&updated).map_err(MetaError::from)?;

                 let compare = if mod_revision == 0 {
                     Compare::version(key, CompareOp::Equal, 0)
                 } else {
                     Compare::mod_revision(key, CompareOp::Equal, mod_revision)
                 };
                 let txn = Txn::new().when([compare]).and_then([TxnOp::put(
                     key,
                     current,
                     options_ref.clone(),
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

        backoff(max_retries.into(), f).await
    }

    /// Get a clone of the etcd client (for Watch Worker)
     pub fn get_client(&self) -> EtcdClient {
         self.client.clone()
     }

     pub fn set_sid(&self, sid: Uuid) -> Result<(), MetaError> {
         self.sid.set(sid).map_err(|_| MetaError::Internal("SID already set".to_string()))
     }

     async fn create_entry(
        &self,
        check_key: &str,
        entries: &[(&str, &[u8])],
        parent: i64,
        name: &str,
    ) -> Result<(), MetaError> {
        let mut client = self.client.clone();
        // Use version instead of create_revision to handle deleted keys correctly
        // version == 0 means the key is currently not present (never existed or deleted)
        let mut txn = Txn::new().when([Compare::version(check_key, CompareOp::Equal, 0)]);
        let mut ops = Vec::new();
        for (key, value) in entries {
            ops.push(TxnOp::put(*key, value.to_vec(), None));
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

        self.update_with_optimistic_concurrency(
            &key,
            None,
            max_retries as u32,
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
        )
        .await
        .map(|_| ())
        .map_err(|e| MetaError::Internal(format!("Update parent children failed: {e}")))
    }

    /// Check file is existing
    async fn file_is_existing(&self, inode: i64) -> Result<bool, MetaError> {
        let key = Self::etcd_reverse_key(inode);

        let entry_info: Option<EtcdEntryInfo> = self.etcd_get(&key).await?;
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
        let put_options = self
            .lease
            .get()
            .map(|lease| PutOptions::new().with_lease(*lease));

        match lock_type {
            FileLockType::UnLock => {
                // Unlock file
                self.update_with_optimistic_concurrency(
                    &key,
                    put_options.clone(),
                    10,
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
                )
                .await
            }
            _ => {
                // Lock request (ReadLock or WriteLock)
                self.update_with_optimistic_concurrency(
                    &key,
                    put_options.clone(),
                    10,
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
                serde_json::from_slice(kv.value()).map_err(MetaError::from)?;

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
                    serde_json::to_vec(&entry_info).map_err(MetaError::from)?,
                    None,
                )])
                .and_then(vec![TxnOp::put(
                    reverse_key.as_bytes(),
                    serde_json::to_vec(&entry_info).map_err(MetaError::from)?,
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

                    return Ok(());
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

        let plocks: Vec<EtcdPlock> = self.etcd_get(&key).await?.unwrap_or_default();

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

#[async_trait]
impl MetaStore for EtcdMetaStore {
    fn name(&self) -> &'static str {
        "etcd"
    }

    async fn stat(&self, ino: i64) -> Result<Option<FileAttr>, MetaError> {
        let reverse_key = Self::etcd_reverse_key(ino);
        let entry_info: Option<EtcdEntryInfo> = self.etcd_get(&reverse_key).await?;
        Ok(entry_info.map(|info| info.to_file_attr(ino)))
    }

    async fn lookup(&self, parent: i64, name: &str) -> Result<Option<i64>, MetaError> {
        let key = Self::etcd_forward_key(parent, name);
        let entry: Option<EtcdForwardEntry> = self.etcd_get(&key).await?;
        Ok(entry.map(|e| e.inode))
    }

    async fn lookup_path(&self, path: &str) -> Result<Option<(i64, FileType)>, MetaError> {
        let path = path.trim();
        if path.is_empty() || path == "/" {
            return Ok(Some((self.root_ino(), FileType::Dir)));
        }

        let mut cur = self.root_ino();
        let mut last_type = FileType::Dir;
        for comp in path.trim_matches('/').split('/') {
            let Some(next) = self.lookup(cur, comp).await? else {
                return Ok(None);
            };

            let Some(attr) = self.stat(next).await? else {
                return Ok(None);
            };

            cur = next;
            last_type = attr.kind;
        }

        Ok(Some((cur, last_type)))
    }

    async fn readdir(&self, ino: i64) -> Result<Vec<DirEntry>, MetaError> {
        let children_key = Self::etcd_children_key(ino);
        let children: Option<EtcdDirChildren> = self.etcd_get(&children_key).await?;
        let Some(children) = children else {
            return Err(MetaError::NotFound(ino));
        };

        let mut entries = Vec::with_capacity(children.children.len());
        for (name, child_ino) in children.children {
            let kind = self
                .stat(child_ino)
                .await?
                .map(|a| a.kind)
                .unwrap_or(FileType::File);

            entries.push(DirEntry {
                name,
                ino: child_ino,
                kind,
            });
        }

        Ok(entries)
    }

    async fn mkdir(&self, parent: i64, name: String) -> Result<i64, MetaError> {
        self.create_directory(parent, name).await
    }

    async fn rmdir(&self, parent: i64, name: &str) -> Result<(), MetaError> {
        let key = Self::etcd_forward_key(parent, name);
        let entry: Option<EtcdForwardEntry> = self.etcd_get(&key).await?;
        let Some(entry) = entry else {
            return Err(MetaError::NotFound(parent));
        };

        let children_key = Self::etcd_children_key(entry.inode);
        let children: Option<EtcdDirChildren> = self.etcd_get(&children_key).await?;
        if let Some(children) = children {
            if !children.children.is_empty() {
                return Err(MetaError::DirectoryNotEmpty(entry.inode));
            }
        }

        let reverse_key = Self::etcd_reverse_key(entry.inode);
        let link_parent_key = Self::etcd_link_parent_key(entry.inode);

        self.delete_entry(
            &key,
            &[key.as_str(), reverse_key.as_str(), children_key.as_str(), link_parent_key.as_str()],
            entry.inode,
        )
        .await?;

        self.update_parent_children(
            parent,
            |children| {
                children.remove(name);
            },
            10,
        )
        .await
    }

    async fn create_file(&self, parent: i64, name: String) -> Result<i64, MetaError> {
        self.create_file_internal(parent, name).await
    }

    async fn unlink(&self, parent: i64, name: &str) -> Result<(), MetaError> {
        let key = Self::etcd_forward_key(parent, name);
        let entry: Option<EtcdForwardEntry> = self.etcd_get(&key).await?;
        let Some(entry) = entry else {
            return Err(MetaError::NotFound(parent));
        };

        let reverse_key = Self::etcd_reverse_key(entry.inode);
        let link_parent_key = Self::etcd_link_parent_key(entry.inode);

        self.delete_entry(
            &key,
            &[key.as_str(), reverse_key.as_str(), link_parent_key.as_str()],
            entry.inode,
        )
        .await?;

        self.update_parent_children(
            parent,
            |children| {
                children.remove(name);
            },
            10,
        )
        .await
    }

    async fn rename(
        &self,
        old_parent: i64,
        old_name: &str,
        new_parent: i64,
        new_name: String,
    ) -> Result<(), MetaError> {
        let old_key = Self::etcd_forward_key(old_parent, old_name);
        let entry: Option<EtcdForwardEntry> = self.etcd_get(&old_key).await?;
        let Some(entry) = entry else {
            return Err(MetaError::NotFound(old_parent));
        };

        let new_key = Self::etcd_forward_key(new_parent, &new_name);
        let reverse_key = Self::etcd_reverse_key(entry.inode);

        let new_forward = EtcdForwardEntry {
            parent_inode: new_parent,
            name: new_name.clone(),
            inode: entry.inode,
            is_file: entry.is_file,
            entry_type: entry.entry_type.clone(),
        };

        self.create_entry(
            &new_key,
            &[(new_key.as_str(), serde_json::to_vec(&new_forward).map_err(MetaError::from)?.as_slice())],
            new_parent,
            &new_name,
        )
        .await?;

        if let Some(mut entry_info) = self.etcd_get::<EtcdEntryInfo>(&reverse_key).await? {
            entry_info.parent_inode = new_parent;
            entry_info.entry_name = new_name.clone();
            self.etcd_put(&reverse_key, &entry_info, None).await?;
        }

        self.delete_entry(&old_key, &[old_key.as_str()], entry.inode).await?;

        self.update_parent_children(
            old_parent,
            |children| {
                children.remove(old_name);
            },
            10,
         )
         .await?;

         let new_name_clone = new_name.clone();
         self.update_parent_children(
             new_parent,
             |children| {
                 children.insert(new_name_clone.clone(), entry.inode);
             },
             10,
         )
         .await
    }

    async fn set_file_size(&self, ino: i64, size: u64) -> Result<(), MetaError> {
        let reverse_key = Self::etcd_reverse_key(ino);
        let mut entry_info: EtcdEntryInfo = self
            .etcd_get(&reverse_key)
            .await?
            .ok_or(MetaError::NotFound(ino))?;

        entry_info.size = Some(size as i64);
        self.etcd_put(&reverse_key, &entry_info, None).await
    }

    async fn get_names(&self, ino: i64) -> Result<Vec<(Option<i64>, String)>, MetaError> {
        let reverse_key = Self::etcd_reverse_key(ino);
        let entry_info: Option<EtcdEntryInfo> = self.etcd_get(&reverse_key).await?;
        let Some(entry_info) = entry_info else {
            return Ok(vec![]);
        };

        Ok(vec![(Some(entry_info.parent_inode), entry_info.entry_name)])
    }

    async fn get_paths(&self, ino: i64) -> Result<Vec<String>, MetaError> {
        let mut parts: Vec<String> = Vec::new();
        let mut cur = ino;

        loop {
            if cur == self.root_ino() {
                break;
            }
            let names = self.get_names(cur).await?;
            let Some((parent, name)) = names.into_iter().next() else {
                break;
            };
            parts.push(name);
            if let Some(p) = parent {
                cur = p;
            } else {
                break;
            }
        }

        parts.reverse();
        Ok(vec![format!("/{}", parts.join("/"))])
    }

    fn root_ino(&self) -> i64 {
        1
    }

    async fn initialize(&self) -> Result<(), MetaError> {
        self.init_root_directory().await
    }

    async fn get_deleted_files(&self) -> Result<Vec<i64>, MetaError> {
        Ok(vec![])
    }

    async fn remove_file_metadata(&self, ino: i64) -> Result<(), MetaError> {
        let reverse_key = Self::etcd_reverse_key(ino);
        let _ = self.client.clone().delete(reverse_key.as_str(), None).await;
        Ok(())
    }

    async fn get_slices(&self, chunk_id: u64) -> Result<Vec<SliceDesc>, MetaError> {
        let key = key_for_slice(chunk_id);
        Ok(self.etcd_get::<Vec<SliceDesc>>(&key).await?.unwrap_or_default())
    }

    async fn append_slice(&self, chunk_id: u64, slice: SliceDesc) -> Result<(), MetaError> {
        let key = key_for_slice(chunk_id);
        self.update_with_optimistic_concurrency(
            &key,
            None,
            10,
            |mut slices: Vec<SliceDesc>| {
                slices.push(slice);
                Ok((slices, ()))
            },
            || Ok((vec![slice], ())),
        )
        .await
        .map(|_| ())
    }

    async fn next_id(&self, key: &str) -> Result<i64, MetaError> {
        self.generate_id(key).await
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
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
    use serial_test::serial;
    use tokio::time;
    use uuid::Uuid;

    async fn cleanup_test_data() -> Result<(), MetaError> {
        use etcd_client::GetOptions;

        let mut client =
            crate::meta::stores::etcd_store::EtcdClient::connect(vec!["127.0.0.1:2379"], None)
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
}
