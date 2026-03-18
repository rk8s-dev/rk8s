//! Etcd-based metadata store implementation
//!
//! Uses Etcd/etcd as the backend for metadata storage

mod txn;
pub(crate) mod watch;

use super::{apply_truncate_plan, build_paths_from_names, trim_slices_in_place};
use crate::chunk::SliceDesc;
use crate::chunk::slice::key_for_slice;
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
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json;
use std::collections::HashMap;
use std::path::Path;
use std::sync::OnceLock;
use tokio::select;
use tokio_util::sync::CancellationToken;
use tracing::{Instrument, error, info, warn};
use uuid::Uuid;

use self::txn::EtcdTxn;

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

impl EtcdMetaStore {
    async fn from_config_inner(config: Config) -> Result<Self, MetaError> {
        info!("Initializing EtcdMetaStore from config");

        let client = Self::create_client(&config).await?;
        let store = Self {
            client,
            _config: config,
            id_pools: IdPool::default(),
            sid: OnceLock::new(),
            lease: OnceLock::new(),
        };
        store.init_root_directory().await?;

        info!("EtcdMetaStore initialized successfully");
        Ok(store)
    }

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

    #[allow(dead_code)]
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
    #[allow(dead_code)]
    pub async fn new(backend_path: &Path) -> Result<Self, MetaError> {
        let config =
            Config::from_path(backend_path).map_err(|e| MetaError::Config(e.to_string()))?;

        info!("Initializing EtcdMetaStore");
        info!("Backend path: {}", backend_path.display());

        Self::from_config_inner(config).await
    }

    /// Create from existing config
    #[allow(dead_code)]
    pub async fn from_config(config: Config) -> Result<Self, MetaError> {
        Self::from_config_inner(config).await
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
    #[cfg(feature = "rkyv-serialization")]
    async fn etcd_get_json<T>(&self, key: &str) -> Result<Option<T>, MetaError>
    where
        T: rkyv::Archive,
        T::Archived:
            rkyv::Deserialize<T, rkyv::rancor::Strategy<rkyv::de::Pool, rkyv::rancor::Error>>,
        for<'de> T: serde::de::DeserializeOwned,
    {
        let mut client = self.client.clone();
        match client.get(key.to_string(), None).await {
            Ok(resp) => {
                if let Some(kv) = resp.kvs().first() {
                    let obj: T = crate::meta::serialization::deserialize_meta(kv.value())?;
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

    #[cfg(not(feature = "rkyv-serialization"))]
    async fn etcd_get_json<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>, MetaError> {
        let mut client = self.client.clone();
        match client.get(key.to_string(), None).await {
            Ok(resp) => {
                if let Some(kv) = resp.kvs().first() {
                    let obj: T = crate::meta::serialization::deserialize_meta(kv.value())?;
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

    async fn etcd_get_json_serde_only<T: DeserializeOwned>(
        &self,
        key: &str,
    ) -> Result<Option<T>, MetaError> {
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

    #[cfg(feature = "rkyv-serialization")]
    async fn etcd_put_json<T>(
        &self,
        key: impl AsRef<str>,
        obj: &T,
        options: Option<PutOptions>,
    ) -> Result<(), MetaError>
    where
        T: rkyv::Archive,
        for<'a> T: rkyv::Serialize<
                rkyv::rancor::Strategy<
                    rkyv::ser::Serializer<
                        rkyv::util::AlignedVec,
                        rkyv::ser::allocator::ArenaHandle<'a>,
                        rkyv::ser::sharing::Share,
                    >,
                    rkyv::rancor::Error,
                >,
            >,
        T: serde::Serialize,
    {
        let mut client = self.client.clone();

        let bytes = crate::meta::serialization::serialize_meta(obj)?;
        let key = key.as_ref();

        client
            .put(key, bytes, options)
            .await
            .map(|_| ())
            .map_err(|e| MetaError::Internal(format!("Failed to put key {key}: {e}")))
    }

    #[cfg(not(feature = "rkyv-serialization"))]
    async fn etcd_put_json<T: Serialize>(
        &self,
        key: impl AsRef<str>,
        obj: &T,
        options: Option<PutOptions>,
    ) -> Result<(), MetaError> {
        let mut client = self.client.clone();

        let bytes = crate::meta::serialization::serialize_meta(obj)?;
        let key = key.as_ref();

        client
            .put(key, bytes, options)
            .await
            .map(|_| ())
            .map_err(|e| MetaError::Internal(format!("Failed to put key {key}: {e}")))
    }

    async fn etcd_put_json_serde_only<T: Serialize>(
        &self,
        key: impl AsRef<str>,
        obj: &T,
        options: Option<PutOptions>,
    ) -> Result<(), MetaError> {
        let mut client = self.client.clone();

        let json = serde_json::to_string(obj).map_err(|e| MetaError::Internal(e.to_string()))?;
        let key = key.as_ref();

        client
            .put(key, json, options)
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
                let chunk_id = chunk_id_for(ino, cutoff_chunk)?;
                let key = key_for_slice(chunk_id);
                let mut slices: Vec<SliceDesc> =
                    self.etcd_get_json(&key).await?.unwrap_or_default();
                trim_slices_in_place(&mut slices, cutoff_offset);
                if slices.is_empty() {
                    let mut client = self.client.clone();
                    client.delete(key.as_str(), None).await.map_err(|e| {
                        MetaError::Internal(format!("Failed to delete key {key}: {e}"))
                    })?;
                } else {
                    self.etcd_put_json(&key, &slices, None).await?;
                }
                Ok(())
            },
            |start, end| async move {
                for idx in start..end {
                    let chunk_id = chunk_id_for(ino, idx)?;
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

    #[cfg(feature = "rkyv-serialization")]
    async fn etcd_get_json_lenient<T>(&self, key: &str) -> Result<Option<T>, MetaError>
    where
        T: rkyv::Archive,
        T::Archived:
            rkyv::Deserialize<T, rkyv::rancor::Strategy<rkyv::de::Pool, rkyv::rancor::Error>>,
        for<'de> T: serde::de::DeserializeOwned,
    {
        match self.etcd_get_json::<T>(key).await {
            Ok(v) => Ok(v),
            Err(e) => {
                error!("Etcd get failed for {}: {}", key, e);
                Ok(None)
            }
        }
    }

    #[cfg(not(feature = "rkyv-serialization"))]
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

    async fn etcd_get_json_lenient_serde_only<T: DeserializeOwned>(
        &self,
        key: &str,
    ) -> Result<Option<T>, MetaError> {
        match self.etcd_get_json_serde_only::<T>(key).await {
            Ok(v) => Ok(v),
            Err(e) => {
                error!("Etcd get failed for {}: {}", key, e);
                Ok(None)
            }
        }
    }

    /// Initialize root directory
    async fn init_root_directory(&self) -> Result<(), MetaError> {
        let now = Utc::now().timestamp_nanos_opt().unwrap_or(0);

        // Create children key for root directory
        let children_key = Self::etcd_children_key(1);
        let root_children = EtcdDirChildren::new(1, HashMap::new());
        let children_json = serde_json::to_string(&root_children)
            .map_err(|e| MetaError::Internal(e.to_string()))?;

        // Create reverse key (metadata) for root directory
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
            parent_inode: 1, // Root's parent is itself
            entry_name: "/".to_string(),
            deleted: false,
            symlink_target: None,
        };
        let reverse_json =
            serde_json::to_string(&root_entry).map_err(|e| MetaError::Internal(e.to_string()))?;

        let mut client = self.client.clone();

        // Atomically create root directory only if it doesn't exist
        // version == 0 means the key is currently not present
        let txn = Txn::new()
            .when([Compare::version(children_key.clone(), CompareOp::Equal, 0)])
            .and_then([
                TxnOp::put(children_key.clone(), children_json, None),
                TxnOp::put(reverse_key, reverse_json, None),
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
        if let Ok(Some(entry_info)) = self
            .etcd_get_json_lenient_serde_only::<EtcdEntryInfo>(&reverse_key)
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
    #[allow(dead_code)]
    async fn get_file_meta(&self, inode: i64) -> Result<Option<FileMetaModel>, MetaError> {
        let reverse_key = Self::etcd_reverse_key(inode);
        if let Ok(Some(entry_info)) = self
            .etcd_get_json_lenient_serde_only::<EtcdEntryInfo>(&reverse_key)
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

        let parent_perm = &parent_meta.permission;
        let parent_has_setgid = (parent_perm.mode & 0o2000) != 0;
        let gid = if parent_has_setgid {
            parent_perm.gid
        } else {
            0
        };

        let dir_permission = Permission::default_directory(0, gid);
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
        let forward_json = serde_json::to_string(&forward_entry)
            .map_err(|e| MetaError::Internal(e.to_string()))?;

        let reverse_key = Self::etcd_reverse_key(inode);
        let reverse_json =
            serde_json::to_string(&entry_info).map_err(|e| MetaError::Internal(e.to_string()))?;

        let children_key = Self::etcd_children_key(inode);
        let children = EtcdDirChildren::new(inode, HashMap::new());
        let children_json =
            serde_json::to_string(&children).map_err(|e| MetaError::Internal(e.to_string()))?;

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
        let forward_json = serde_json::to_string(&forward_entry)
            .map_err(|e| MetaError::Internal(e.to_string()))?;

        let reverse_key = Self::etcd_reverse_key(inode);
        let reverse_json =
            serde_json::to_string(&entry_info).map_err(|e| MetaError::Internal(e.to_string()))?;

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

        if let Some(id) = self.id_pools.try_alloc(counter_key) {
            return Ok(id);
        }

        // Slow path: pool exhausted, need to allocate new batch from etcd
        info!(
            counter_key = counter_key,
            pool_hit = false,
            "Pool exhausted, allocating new batch from etcd"
        );

        let counter_key = counter_key.to_string();

        let (allocated_id, next_start, pool_end) = EtcdTxn::new(&self.client)
            .max_retries(10)
            .run(|tx| {
                let counter_key = counter_key.clone();

                Box::pin(async move {
                    let current_id = tx.get_typed_json::<i64>(&counter_key).await?;

                    let (current_id, next_etcd_id) = if let Some(current_id) = current_id {
                        let next_etcd_id = current_id.checked_add(BATCH_SIZE).ok_or_else(|| {
                            MetaError::Internal("ID counter overflow".to_string())
                        })?;

                        (current_id, next_etcd_id)
                    } else {
                        let next_etcd_id =
                            FIRST_ALLOCATED_ID.checked_add(BATCH_SIZE).ok_or_else(|| {
                                MetaError::Internal("ID counter overflow".to_string())
                            })?;

                        (FIRST_ALLOCATED_ID, next_etcd_id)
                    };

                    tx.set_typed_json(&counter_key, &next_etcd_id)?;

                    Ok((current_id, current_id + 1, next_etcd_id))
                })
            })
            .await?;

        self.id_pools.update(&counter_key, next_start, pool_end);

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
        let check_key = check_key.to_string();
        let name = name.to_string();
        let entries: Vec<(String, Vec<u8>)> = entries
            .iter()
            .map(|(key, value)| ((*key).to_string(), value.as_bytes().to_vec()))
            .collect();

        EtcdTxn::new(&self.client)
            .max_retries(10)
            .run(|tx| {
                let check_key = check_key.clone();
                let entries = entries.clone();
                let name = name.clone();

                Box::pin(async move {
                    if tx.exists(&check_key).await? {
                        return Err(MetaError::AlreadyExists { parent, name });
                    }

                    for (key, value) in entries {
                        tx.set(key, value);
                    }

                    Ok(())
                })
            })
            .await
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
        let check_key = check_key.to_string();
        let keys: Vec<String> = keys.iter().map(|key| (*key).to_string()).collect();

        EtcdTxn::new(&self.client)
            .max_retries(10)
            .run(|tx| {
                let check_key = check_key.clone();
                let keys = keys.clone();

                Box::pin(async move {
                    if !tx.exists(&check_key).await? {
                        return Err(MetaError::NotFound(ino));
                    }

                    for key in keys {
                        tx.delete(key);
                    }

                    Ok(())
                })
            })
            .await
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

        EtcdTxn::new(&self.client)
            .max_retries(max_retries as u64)
            .run(|tx| {
                let key = key.clone();
                let updater = &updater;

                Box::pin(async move {
                    let dir = tx.get_typed_json::<EtcdDirChildren>(&key).await?;

                    let mut children = dir.map(|dir| dir.children).unwrap_or_default();
                    updater(&mut children);

                    let dir = EtcdDirChildren::new(parent_ino, children);
                    tx.set_typed_json(&key, &dir)?;

                    Ok(())
                })
            })
            .await
            .map_err(|e| MetaError::Internal(format!("Update parent children failed: {e}")))
    }

    /// Check file is existing
    #[allow(dead_code)]
    async fn file_is_existing(&self, inode: i64) -> Result<bool, MetaError> {
        let key = Self::etcd_reverse_key(inode);

        let entry_info: Option<EtcdEntryInfo> = self.etcd_get_json_serde_only(&key).await?;
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
                EtcdTxn::new(&self.client)
                    .max_retries(10)
                    .run(|tx| {
                        let key = key.clone();
                        let put_options = put_options.clone();

                        Box::pin(async move {
                            let mut plocks = tx
                                .get_typed_json::<Vec<EtcdPlock>>(&key)
                                .await?
                                .unwrap_or_default();

                            if let Some(pos) = plocks
                                .iter()
                                .position(|p| p.sid == *sid && p.owner == owner)
                            {
                                let records = plocks[pos].records.clone();

                                if records.is_empty() {
                                    plocks.remove(pos);
                                } else {
                                    let new_records = PlockRecord::update_locks(records, *new_lock);

                                    if new_records.is_empty() {
                                        plocks.remove(pos);
                                    } else {
                                        plocks[pos].records = new_records;
                                    }
                                }
                            }

                            tx.set_with_options(
                                key,
                                serde_json::to_vec(&plocks)
                                    .map_err(|e| MetaError::Internal(e.to_string()))?,
                                put_options,
                            );

                            Ok(())
                        })
                    })
                    .await
            }
            _ => {
                EtcdTxn::new(&self.client)
                    .max_retries(10)
                    .run(|tx| {
                        let key = key.clone();
                        let put_options = put_options.clone();

                        Box::pin(async move {
                            let mut plocks = tx
                                .get_typed_json::<Vec<EtcdPlock>>(&key)
                                .await?
                                .unwrap_or_default();

                            let mut locks = HashMap::new();
                            for item in &plocks {
                                let key = (item.sid, item.owner);
                                locks.insert(key, item.records.clone());
                            }

                            let lock_key = (*sid, owner);

                            for ((lock_sid, _), records) in &locks {
                                if (*lock_sid, owner) == lock_key {
                                    continue;
                                }

                                if PlockRecord::check_conflict(&lock_type, &range, records) {
                                    return Err(MetaError::LockConflict {
                                        inode,
                                        owner,
                                        range,
                                    });
                                }
                            }

                            let records = locks.get(&lock_key).cloned().unwrap_or_default();
                            let records = PlockRecord::update_locks(records, *new_lock);

                            if locks
                                .get(&lock_key)
                                .map(|existing| existing != &records)
                                .unwrap_or(true)
                            {
                                if let Some(plock) = plocks
                                    .iter_mut()
                                    .find(|p| p.sid == *sid && p.owner == owner)
                                {
                                    plock.records = records;
                                } else {
                                    plocks.push(EtcdPlock {
                                        sid: *sid,
                                        owner,
                                        records,
                                    });
                                }
                            }

                            tx.set_with_options(
                                key,
                                serde_json::to_vec(&plocks)
                                    .map_err(|e| MetaError::Internal(e.to_string()))?,
                                put_options,
                            );

                            Ok(())
                        })
                    })
                    .await
            }
        }
    }

    /// Update mtime and ctime for a directory inode
    async fn update_directory_timestamps(&self, ino: i64, now: i64) -> Result<(), MetaError> {
        let reverse_key = Self::etcd_reverse_key(ino);

        EtcdTxn::new(&self.client)
            .max_retries(10)
            .run(|tx| {
                let reverse_key = reverse_key.clone();

                Box::pin(async move {
                    let mut entry_info: EtcdEntryInfo = tx
                        .get_typed_json(&reverse_key)
                        .await?
                        .ok_or(MetaError::NotFound(ino))?;

                    if entry_info.is_file {
                        return Err(MetaError::Internal(format!(
                            "Cannot update directory timestamps for file {ino}"
                        )));
                    }

                    entry_info.modify_time = now;
                    entry_info.create_time = now;

                    tx.set_typed_json(reverse_key, &entry_info)?;

                    Ok(())
                })
            })
            .await
    }
    #[allow(dead_code)]
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
    #[allow(dead_code)]
    fn get_sid(&self) -> Result<&Uuid, MetaError> {
        self.sid
            .get()
            .ok_or_else(|| MetaError::Internal("sid has not been set".to_string()))
    }
    #[allow(dead_code)]
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
    fn name(&self) -> &'static str {
        "etcd"
    }

    async fn from_config(config: Config) -> Result<Self, MetaError> {
        Self::from_config_inner(config).await
    }

    #[tracing::instrument(level = "trace", skip(self), fields(ino))]
    async fn stat(&self, ino: i64) -> Result<Option<FileAttr>, MetaError> {
        // Query reverse index once to get all metadata
        let reverse_key = Self::etcd_reverse_key(ino);
        if let Ok(Some(entry_info)) = self
            .etcd_get_json_lenient_serde_only::<EtcdEntryInfo>(&reverse_key)
            .await
        {
            return Ok(Some(entry_info.to_file_attr(ino)));
        }

        Ok(None)
    }

    /// Batch stat implementation for Etcd using Transaction batch GET
    /// Uses single transaction to fetch multiple keys - much faster than sequential queries
    #[tracing::instrument(
        level = "trace",
        skip(self, inodes),
        fields(inode_count = inodes.len())
    )]
    async fn batch_stat(&self, inodes: &[i64]) -> Result<Vec<Option<FileAttr>>, MetaError> {
        if inodes.is_empty() {
            return Ok(Vec::new());
        }

        // Etcd transaction has ~128 operations limit, process in chunks
        const MAX_KEYS_PER_TXN: usize = 100;

        let mut results: Vec<Option<FileAttr>> = vec![None; inodes.len()];

        // Process in chunks to respect Etcd transaction limits
        for (chunk_idx, chunk) in inodes.chunks(MAX_KEYS_PER_TXN).enumerate() {
            let chunk_offset = chunk_idx * MAX_KEYS_PER_TXN;

            // Build transaction with GET operations for all inodes in chunk
            let mut get_ops = Vec::new();
            for &ino in chunk {
                let reverse_key = Self::etcd_reverse_key(ino);
                get_ops.push(TxnOp::get(reverse_key.as_bytes(), None));
            }

            // Execute transaction - all GETs in single round trip
            let mut client_clone = self.client.clone();
            let txn = Txn::new().and_then(get_ops);
            let txn_response = client_clone
                .txn(txn)
                .await
                .map_err(|e| MetaError::Internal(format!("Etcd batch txn error: {}", e)))?;

            // Etcd preserves response order for each request op in the txn success list.
            let responses = txn_response.op_responses();

            // Parse responses - one response per inode
            for (i, &ino) in chunk.iter().enumerate() {
                let result_idx = chunk_offset + i;

                // Handle special case for root inode
                if ino == 1 {
                    let now = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0);
                    results[result_idx] = Some(FileAttr {
                        ino: 1,
                        size: 4096,
                        kind: FileType::Dir,
                        mode: 0o40755,
                        uid: 0,
                        gid: 0,
                        atime: now,
                        mtime: now,
                        ctime: now,
                        nlink: 2,
                    });
                    continue;
                }

                // Get response for this inode
                if let Some(resp) = responses.get(i) {
                    // TxnOpResponse is an enum, match to extract GetResponse
                    if let TxnOpResponse::Get(range_resp) = resp
                        && let Some(kv) = range_resp.kvs().first()
                    {
                        // Parse EtcdEntryInfo from the value
                        if let Ok(entry_info) = serde_json::from_slice::<EtcdEntryInfo>(kv.value())
                        {
                            results[result_idx] = Some(entry_info.to_file_attr(ino));
                        }
                    }
                }
            }
        }

        Ok(results)
    }

    #[tracing::instrument(level = "trace", skip(self), fields(parent, name))]
    async fn lookup(&self, parent: i64, name: &str) -> Result<Option<i64>, MetaError> {
        let forward_key = Self::etcd_forward_key(parent, name);
        if let Some(entry) = self.etcd_get_json::<EtcdForwardEntry>(&forward_key).await? {
            Ok(Some(entry.inode))
        } else {
            Ok(None)
        }
    }

    #[tracing::instrument(level = "trace", skip(self), fields(path))]
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

    #[tracing::instrument(level = "trace", skip(self), fields(ino))]
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

    #[tracing::instrument(level = "trace", skip(self), fields(parent, name))]
    async fn mkdir(&self, parent: i64, name: String) -> Result<i64, MetaError> {
        self.create_directory(parent, name).await
    }

    #[tracing::instrument(level = "trace", skip(self), fields(parent, name))]
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

    #[tracing::instrument(level = "trace", skip(self), fields(parent, name))]
    async fn create_file(&self, parent: i64, name: String) -> Result<i64, MetaError> {
        self.create_file_internal(parent, name).await
    }

    #[tracing::instrument(level = "trace", skip(self), fields(ino, parent, name))]
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

        let name = name.to_string();
        let reverse_key = Self::etcd_reverse_key(ino);
        let forward_key = Self::etcd_forward_key(parent, &name);
        let link_parent_key = Self::etcd_link_parent_key(ino);

        info!(
            "Creating hard link with atomic transaction: src_inode={}, parent={}, name={} ",
            ino, parent, name
        );

        let attr = EtcdTxn::new(&self.client)
            .max_retries(10)
            .run(|tx| {
                let reverse_key = reverse_key.clone();
                let forward_key = forward_key.clone();
                let link_parent_key = link_parent_key.clone();
                let name = name.clone();

                Box::pin(async move {
                    if tx.exists(&forward_key).await? {
                        return Err(MetaError::AlreadyExists {
                            parent,
                            name: name.clone(),
                        });
                    }

                    let mut entry_info: EtcdEntryInfo = tx
                        .get_typed_json(&reverse_key)
                        .await?
                        .ok_or(MetaError::NotFound(ino))?;

                    if !entry_info.is_file {
                        return Err(MetaError::NotSupported(
                            "cannot create hard links to directories".into(),
                        ));
                    }

                    if entry_info.symlink_target.is_some() {
                        return Err(MetaError::NotSupported(
                            "cannot create hard links to symbolic links".into(),
                        ));
                    }

                    if entry_info.deleted || entry_info.nlink == 0 {
                        return Err(MetaError::NotFound(ino));
                    }

                    let old_nlink = entry_info.nlink;
                    let now = Utc::now().timestamp_nanos_opt().unwrap_or(0);

                    entry_info.nlink = entry_info.nlink.saturating_add(1);
                    entry_info.modify_time = now;
                    entry_info.create_time = now;
                    entry_info.deleted = false;

                    if old_nlink == 1 {
                        if tx.exists(&link_parent_key).await? {
                            return Err(MetaError::Internal(format!(
                                "LinkParent key {} unexpectedly exists for inode {}",
                                link_parent_key, ino
                            )));
                        }

                        let old_parent = entry_info.parent_inode;
                        let old_entry_name = entry_info.entry_name.clone();

                        entry_info.parent_inode = 0;
                        entry_info.entry_name = String::new();

                        let link_parents = vec![
                            EtcdLinkParent {
                                parent_inode: old_parent,
                                entry_name: old_entry_name,
                            },
                            EtcdLinkParent {
                                parent_inode: parent,
                                entry_name: name.clone(),
                            },
                        ];

                        tx.set_typed_json(&link_parent_key, &link_parents)?;
                    } else {
                        let mut link_parents: Vec<EtcdLinkParent> =
                            tx.get_typed_json(&link_parent_key).await?.ok_or_else(|| {
                                MetaError::Internal(format!(
                                    "LinkParent key {} not found for inode {}",
                                    link_parent_key, ino
                                ))
                            })?;

                        link_parents.push(EtcdLinkParent {
                            parent_inode: parent,
                            entry_name: name.clone(),
                        });

                        tx.set_typed_json(&link_parent_key, &link_parents)?;
                    }

                    let forward_entry = EtcdForwardEntry {
                        parent_inode: parent,
                        name: name.clone(),
                        inode: ino,
                        is_file: true,
                        entry_type: Some(EntryType::File),
                    };

                    tx.set_typed_json(&forward_key, &forward_entry)?;
                    tx.set_typed_json(&reverse_key, &entry_info)?;

                    Ok(entry_info.to_file_attr(ino))
                })
            })
            .await?;

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

        Ok(attr)
    }

    #[tracing::instrument(level = "trace", skip(self), fields(parent, name, target))]
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

    #[tracing::instrument(level = "trace", skip(self), fields(ino))]
    async fn read_symlink(&self, ino: i64) -> Result<String, MetaError> {
        let reverse_key = Self::etcd_reverse_key(ino);
        let entry_info = self
            .etcd_get_json_serde_only::<EtcdEntryInfo>(&reverse_key)
            .await?
            .ok_or(MetaError::NotFound(ino))?;

        if entry_info.deleted {
            return Err(MetaError::NotFound(ino));
        }

        entry_info
            .symlink_target
            .ok_or_else(|| MetaError::NotSupported(format!("inode {ino} is not a symbolic link")))
    }

    #[tracing::instrument(level = "trace", skip(self), fields(parent, name))]
    async fn unlink(&self, parent: i64, name: &str) -> Result<(), MetaError> {
        let name = name.to_string();
        let forward_key = Self::etcd_forward_key(parent, &name);

        let file_ino = EtcdTxn::new(&self.client)
            .max_retries(10)
            .run(|tx| {
                let forward_key = forward_key.clone();
                let name = name.clone();

                Box::pin(async move {
                    let forward_entry: EtcdForwardEntry = tx
                        .get_typed_json(&forward_key)
                        .await?
                        .ok_or(MetaError::NotFound(parent))?;

                    if !forward_entry.is_file {
                        return Err(MetaError::Internal("Is a directory".to_string()));
                    }

                    let file_ino = forward_entry.inode;
                    let reverse_key = Self::etcd_reverse_key(file_ino);
                    let mut entry_info: EtcdEntryInfo = tx
                        .get_typed_json(&reverse_key)
                        .await?
                        .ok_or(MetaError::NotFound(file_ino))?;

                    let current_nlink = entry_info.nlink;
                    let now = Utc::now().timestamp_nanos_opt().unwrap_or(0);

                    if current_nlink > 1 {
                        let link_parent_key = Self::etcd_link_parent_key(file_ino);
                        let mut link_parents: Vec<EtcdLinkParent> = tx
                            .get_typed_json(&link_parent_key)
                            .await?
                            .ok_or_else(|| {
                                MetaError::Internal(format!(
                                    "LinkParent key {} not found for inode {}",
                                    link_parent_key, file_ino
                                ))
                            })?;

                        let original_len = link_parents.len();
                        link_parents
                            .retain(|lp| lp.parent_inode != parent || lp.entry_name != name);

                        if link_parents.len() == original_len {
                            return Err(MetaError::Internal(format!(
                                "No LinkParent entry found for parent {} name {} inode {}",
                                parent, name, file_ino
                            )));
                        }

                        if current_nlink == 2 {
                            let remaining = link_parents.first().ok_or_else(|| {
                                MetaError::Internal(format!(
                                    "No remaining LinkParent found for inode {} during 2->1 transition",
                                    file_ino
                                ))
                            })?;

                            entry_info.parent_inode = remaining.parent_inode;
                            entry_info.entry_name = remaining.entry_name.clone();
                            entry_info.nlink = 1;
                            entry_info.deleted = false;

                            tx.delete(link_parent_key);
                        } else {
                            entry_info.nlink = current_nlink - 1;
                            entry_info.deleted = false;

                            tx.set_typed_json(link_parent_key, &link_parents)?;
                        }
                    } else {
                        entry_info.deleted = true;
                        entry_info.nlink = 0;
                        entry_info.parent_inode = 0;
                    }

                    entry_info.modify_time = now;

                    tx.delete(forward_key);
                    tx.set_typed_json(reverse_key, &entry_info)?;

                    Ok(file_ino)
                })
            })
            .await?;

        info!(
            "File unlink transaction succeeded: parent={}, name={}, inode={}",
            parent, name, file_ino
        );

        let name_for_closure = name.clone();
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
                warn!(
                    "Unlink succeeded but failed to update parent children map: parent={}, name={}, inode={}, error={}. Forward key deleted, lookup will fail correctly.",
                    parent, name, file_ino, e
                );
                Ok(())
            }
        }
    }

    #[tracing::instrument(
        level = "trace",
        skip(self),
        fields(old_parent, old_name, new_parent, new_name)
    )]
    async fn rename(
        &self,
        old_parent: i64,
        old_name: &str,
        new_parent: i64,
        new_name: String,
    ) -> Result<(), MetaError> {
        let old_forward_key = Self::etcd_forward_key(old_parent, old_name);
        let new_forward_key = Self::etcd_forward_key(new_parent, &new_name);
        info!(
            "Renaming with transaction: {} (parent={}) -> {} (parent={})",
            old_name, old_parent, new_name, new_parent
        );

        let entry_ino = EtcdTxn::new(&self.client)
            .max_retries(10)
            .run(|tx| {
                let old_forward_key = old_forward_key.clone();
                let new_forward_key = new_forward_key.clone();
                let old_name = old_name.to_string();
                let new_name = new_name.clone();

                Box::pin(async move {
                    let old_forward_entry: EtcdForwardEntry = tx
                        .get_typed_json(&old_forward_key)
                        .await?
                        .ok_or(MetaError::NotFound(old_parent))?;

                    if tx.exists(&new_forward_key).await? {
                        return Err(MetaError::AlreadyExists {
                            parent: new_parent,
                            name: new_name.clone(),
                        });
                    }

                    let entry_ino = old_forward_entry.inode;
                    let reverse_key = Self::etcd_reverse_key(entry_ino);
                    let mut entry_info: EtcdEntryInfo = tx
                        .get_typed_json(&reverse_key)
                        .await?
                        .ok_or(MetaError::NotFound(entry_ino))?;

                    if entry_info.nlink <= 1 {
                        entry_info.parent_inode = new_parent;
                        entry_info.entry_name = new_name.clone();
                    } else {
                        let link_parent_key = Self::etcd_link_parent_key(entry_ino);
                        let mut link_parents: Vec<EtcdLinkParent> = tx
                            .get_typed_json(&link_parent_key)
                            .await?
                            .unwrap_or_default();

                        let mut updated = false;
                        for link_parent in &mut link_parents {
                            if link_parent.parent_inode == old_parent
                                && link_parent.entry_name == old_name
                            {
                                link_parent.parent_inode = new_parent;
                                link_parent.entry_name = new_name.clone();
                                updated = true;
                                break;
                            }
                        }

                        if !updated {
                            return Err(MetaError::NotFound(entry_ino));
                        }

                        tx.set_typed_json(link_parent_key, &link_parents)?;
                    }

                    entry_info.modify_time = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0);

                    let new_forward_entry = EtcdForwardEntry {
                        parent_inode: new_parent,
                        name: new_name.clone(),
                        inode: entry_ino,
                        is_file: old_forward_entry.is_file,
                        entry_type: old_forward_entry.entry_type.clone(),
                    };

                    tx.set_typed_json(&new_forward_key, &new_forward_entry)?;
                    tx.delete(&old_forward_key);
                    tx.set_typed_json(&reverse_key, &entry_info)?;

                    Ok(entry_ino)
                })
            })
            .await?;

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

    async fn rename_exchange(
        &self,
        old_parent: i64,
        old_name: &str,
        new_parent: i64,
        new_name: &str,
    ) -> Result<(), MetaError> {
        let old_forward_key = Self::etcd_forward_key(old_parent, old_name);
        let new_forward_key = Self::etcd_forward_key(new_parent, new_name);

        EtcdTxn::new(&self.client)
            .max_retries(10)
            .run(|tx| {
                let old_forward_key = old_forward_key.clone();
                let new_forward_key = new_forward_key.clone();
                let old_name = old_name.to_string();
                let new_name = new_name.to_string();

                Box::pin(async move {
                    let old_forward_entry: EtcdForwardEntry =
                        tx.get_typed_json(&old_forward_key).await?.ok_or_else(|| {
                            MetaError::Internal(format!(
                                "Entry '{}' not found in parent {} for exchange",
                                old_name, old_parent
                            ))
                        })?;

                    let new_forward_entry: EtcdForwardEntry =
                        tx.get_typed_json(&new_forward_key).await?.ok_or_else(|| {
                            MetaError::Internal(format!(
                                "Entry '{}' not found in parent {} for exchange",
                                new_name, new_parent
                            ))
                        })?;

                    let swapped_old_forward = EtcdForwardEntry {
                        parent_inode: old_parent,
                        name: old_name.clone(),
                        inode: new_forward_entry.inode,
                        is_file: new_forward_entry.is_file,
                        entry_type: new_forward_entry.entry_type.clone(),
                    };

                    let swapped_new_forward = EtcdForwardEntry {
                        parent_inode: new_parent,
                        name: new_name.clone(),
                        inode: old_forward_entry.inode,
                        is_file: old_forward_entry.is_file,
                        entry_type: old_forward_entry.entry_type.clone(),
                    };

                    tx.set_typed_json(&old_forward_key, &swapped_old_forward)?;
                    tx.set_typed_json(&new_forward_key, &swapped_new_forward)?;

                    Ok(())
                })
            })
            .await?;

        info!(
            "Exchange completed successfully: ({}, '{}') <-> ({}, '{}')",
            old_parent, old_name, new_parent, new_name
        );

        Ok(())
    }

    #[tracing::instrument(level = "trace", skip(self), fields(ino, size))]
    async fn set_file_size(&self, ino: i64, size: u64) -> Result<(), MetaError> {
        let reverse_key = Self::etcd_reverse_key(ino);

        EtcdTxn::new(&self.client)
            .max_retries(10)
            .run(|tx| {
                let reverse_key = reverse_key.clone();

                Box::pin(async move {
                    let mut entry_info: EtcdEntryInfo = tx
                        .get_typed_json(&reverse_key)
                        .await?
                        .ok_or(MetaError::NotFound(ino))?;

                    if !entry_info.is_file {
                        return Err(MetaError::Internal(
                            "Cannot set size for directory".to_string(),
                        ));
                    }

                    entry_info.size = Some(size as i64);
                    entry_info.modify_time = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0);
                    tx.set_typed_json(reverse_key, &entry_info)?;

                    Ok(())
                })
            })
            .await
    }

    #[tracing::instrument(level = "trace", skip(self), fields(ino, size))]
    async fn extend_file_size(&self, ino: i64, size: u64) -> Result<(), MetaError> {
        let reverse_key = Self::etcd_reverse_key(ino);

        EtcdTxn::new(&self.client)
            .max_retries(10)
            .run(|tx| {
                let reverse_key = reverse_key.clone();

                Box::pin(async move {
                    let mut entry_info: EtcdEntryInfo = tx
                        .get_typed_json(&reverse_key)
                        .await?
                        .ok_or(MetaError::NotFound(ino))?;

                    if !entry_info.is_file {
                        return Err(MetaError::Internal(
                            "Cannot set size for directory".to_string(),
                        ));
                    }

                    let current = entry_info.size.unwrap_or(0) as u64;
                    if size > current {
                        entry_info.size = Some(size as i64);
                        entry_info.modify_time =
                            chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0);
                        tx.set_typed_json(reverse_key, &entry_info)?;
                    }

                    Ok(())
                })
            })
            .await
    }

    #[tracing::instrument(level = "trace", skip(self), fields(ino, size, chunk_size))]
    async fn truncate(&self, ino: i64, size: u64, chunk_size: u64) -> Result<(), MetaError> {
        let reverse_key = Self::etcd_reverse_key(ino);

        let old_size = EtcdTxn::new(&self.client)
            .max_retries(10)
            .run(|tx| {
                let reverse_key = reverse_key.clone();

                Box::pin(async move {
                    let mut entry_info: EtcdEntryInfo = tx
                        .get_typed_json(&reverse_key)
                        .await?
                        .ok_or(MetaError::NotFound(ino))?;

                    if !entry_info.is_file {
                        return Err(MetaError::Internal(
                            "Cannot set size for directory".to_string(),
                        ));
                    }

                    let prev = entry_info.size.unwrap_or(0) as u64;
                    entry_info.size = Some(size as i64);
                    entry_info.modify_time = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0);
                    tx.set_typed_json(reverse_key, &entry_info)?;

                    Ok(prev)
                })
            })
            .await?;

        self.prune_slices_for_truncate(ino, size, old_size, chunk_size)
            .await?;
        Ok(())
    }

    #[tracing::instrument(level = "trace", skip(self), fields(ino))]
    async fn get_names(&self, ino: i64) -> Result<Vec<(Option<i64>, String)>, MetaError> {
        if ino == 1 {
            return Ok(vec![(None, "/".to_string())]);
        }

        let reverse_key = Self::etcd_reverse_key(ino);
        let Some(entry_info) = self
            .etcd_get_json_serde_only::<EtcdEntryInfo>(&reverse_key)
            .await?
        else {
            return Ok(vec![]);
        };

        if entry_info.deleted || entry_info.nlink == 0 {
            return Ok(vec![]);
        }

        if !entry_info.is_file || entry_info.nlink <= 1 {
            return Ok(vec![(Some(entry_info.parent_inode), entry_info.entry_name)]);
        }

        let link_parent_key = Self::etcd_link_parent_key(ino);
        let link_parents = self
            .etcd_get_json::<Vec<EtcdLinkParent>>(&link_parent_key)
            .await?
            .unwrap_or_default();

        let mut out = Vec::with_capacity(link_parents.len());
        for lp in link_parents {
            out.push((Some(lp.parent_inode), lp.entry_name));
        }

        out.sort();
        out.dedup();
        Ok(out)
    }

    #[tracing::instrument(level = "trace", skip(self), fields(ino))]
    async fn get_paths(&self, ino: i64) -> Result<Vec<String>, MetaError> {
        if ino == 1 {
            return Ok(vec!["/".to_string()]);
        }

        let names = self.get_names(ino).await?;

        build_paths_from_names(1, names, |current_ino| async move {
            let reverse_key = Self::etcd_reverse_key(current_ino);
            let entry_info = self
                .etcd_get_json_serde_only::<EtcdEntryInfo>(&reverse_key)
                .await?;

            Ok(entry_info.map(|entry_info| (entry_info.parent_inode, entry_info.entry_name)))
        })
        .await
    }

    fn root_ino(&self) -> i64 {
        1
    }

    #[tracing::instrument(level = "trace", skip(self))]
    async fn initialize(&self) -> Result<(), MetaError> {
        Ok(())
    }

    #[tracing::instrument(level = "trace", skip(self))]
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

    #[tracing::instrument(level = "trace", skip(self), fields(ino))]
    async fn remove_file_metadata(&self, ino: i64) -> Result<(), MetaError> {
        let mut client = self.client.clone();

        let reverse_key = Self::etcd_reverse_key(ino);

        // Check if the file exists and is marked as deleted
        let entry_info: EtcdEntryInfo = match self
            .etcd_get_json_serde_only::<EtcdEntryInfo>(&reverse_key)
            .await?
        {
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

    #[tracing::instrument(
        level = "trace",
        skip(self),
        fields(chunk_id, slice_count = tracing::field::Empty)
    )]
    async fn get_slices(&self, chunk_id: u64) -> Result<Vec<SliceDesc>, MetaError> {
        let key = key_for_slice(chunk_id);
        let slices: Vec<SliceDesc> = self
            .etcd_get_json(&key)
            .instrument(tracing::trace_span!("get_slices.etcd_get", key = %key))
            .await?
            .unwrap_or_default();
        tracing::Span::current().record("slice_count", slices.len());
        Ok(slices)
    }

    #[tracing::instrument(
        level = "trace",
        skip(self, slice),
        fields(chunk_id, slice_id = slice.slice_id, offset = slice.offset, len = slice.length)
    )]
    async fn append_slice(&self, chunk_id: u64, slice: SliceDesc) -> Result<(), MetaError> {
        let key = key_for_slice(chunk_id);

        EtcdTxn::new(&self.client)
            .max_retries(10)
            .run(|tx| {
                let key = key.clone();

                Box::pin(async move {
                    let mut source: Vec<SliceDesc> = tx.get_typed(&key).await?.unwrap_or_default();
                    source.push(slice);
                    tx.set_typed(key, &source)?;

                    Ok(())
                })
            })
            .await
    }

    async fn write(
        &self,
        ino: i64,
        chunk_id: u64,
        slice: SliceDesc,
        new_size: u64,
    ) -> Result<(), MetaError> {
        let slice_key = key_for_slice(chunk_id);
        let inode_key = Self::etcd_reverse_key(ino);

        EtcdTxn::new(&self.client)
            .max_retries(10)
            .run(|tx| {
                let slice_key = slice_key.clone();
                let inode_key = inode_key.clone();

                Box::pin(async move {
                    let mut slices: Vec<SliceDesc> =
                        tx.get_typed(&slice_key).await?.unwrap_or_default();
                    slices.push(slice);

                    tx.set_typed(slice_key, &slices)?;

                    let mut entry_info: EtcdEntryInfo = tx
                        .get_typed_json(&inode_key)
                        .await?
                        .ok_or(MetaError::NotFound(ino))?;

                    if !entry_info.is_file {
                        return Err(MetaError::Internal(
                            "Cannot set size for directory".to_string(),
                        ));
                    }

                    let current = entry_info.size.unwrap_or(0).max(0) as u64;
                    if new_size > current {
                        entry_info.size = Some(new_size as i64);
                        entry_info.modify_time =
                            chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0);
                        // POSIX: clear setuid/setgid bits on write (security: prevent privilege escalation)
                        entry_info.permission.mode &= !0o6000;

                        tx.set_typed_json(inode_key, &entry_info)?;
                    }

                    Ok(())
                })
            })
            .await
    }

    #[tracing::instrument(level = "trace", skip(self), fields(key))]
    async fn next_id(&self, key: &str) -> Result<i64, MetaError> {
        self.generate_id(key).await
    }

    // ---------- Session lifecycle implementation ----------

    #[tracing::instrument(level = "trace", skip(self), fields(pid = session_info.process_id))]
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
        self.etcd_put_json_serde_only(session_info_key, &session_info, Some(options.clone()))
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

    #[tracing::instrument(level = "trace", skip(self))]
    async fn shutdown_session(&self) -> Result<(), MetaError> {
        let session_id = *self.get_sid()?;
        self.shutdown_session_by_id(session_id).await?;
        Ok(())
    }

    // Etcd cleanup is performed by the lease keeper
    #[tracing::instrument(level = "trace", skip(self))]
    async fn cleanup_sessions(&self) -> Result<(), MetaError> {
        return Ok(());
    }
    #[tracing::instrument(level = "trace", skip(self), fields(lock_name = ?lock_name))]
    async fn get_global_lock(&self, lock_name: LockName) -> bool {
        let lock_key = lock_name.to_string();
        let result = EtcdTxn::new(&self.client)
            .max_retries(3)
            .run(|tx| {
                let lock_key = lock_key.clone();

                Box::pin(async move {
                    let now = Utc::now().timestamp_millis();
                    let current = tx.get_typed_json::<i64>(&lock_key).await?;

                    let (updated, acquired) = if let Some(current) = current {
                        if now > current + Duration::seconds(7).num_milliseconds() {
                            (now, true)
                        } else {
                            (current, false)
                        }
                    } else {
                        (now, true)
                    };

                    tx.set_typed_json(&lock_key, &updated)?;

                    Ok(acquired)
                })
            })
            .await;

        match result {
            Ok(flag) => flag,
            Err(err) => {
                error!("Error getting lock: {}", err);
                false
            }
        }
    }

    #[tracing::instrument(
        level = "trace",
        skip(self, req),
        fields(ino, size = req.size, flags = ?flags)
    )]
    async fn set_attr(
        &self,
        ino: i64,
        req: &SetAttrRequest,
        flags: SetAttrFlags,
    ) -> Result<FileAttr, MetaError> {
        let reverse_key = Self::etcd_reverse_key(ino);
        let now = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0);
        let req = *req;
        let flag_bits = flags.bits();

        EtcdTxn::new(&self.client)
            .max_retries(10)
            .run(|tx| {
                let reverse_key = reverse_key.clone();
                let flags = SetAttrFlags::from_bits_retain(flag_bits);

                Box::pin(async move {
                    let mut entry_info: EtcdEntryInfo = tx
                        .get_typed_json(&reverse_key)
                        .await?
                        .ok_or(MetaError::NotFound(ino))?;

                    let mut ctime_update = false;

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

                    if flags.contains(SetAttrFlags::CLEAR_SUID) {
                        entry_info.permission.mode &= !0o4000;
                        ctime_update = true;
                    }

                    if flags.contains(SetAttrFlags::CLEAR_SGID) {
                        entry_info.permission.mode &= !0o2000;
                        ctime_update = true;
                    }

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

                    tx.set_typed_json(reverse_key, &entry_info)?;

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

                    Ok(FileAttr {
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
                    })
                })
            })
            .await
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    // returns the current lock owner for a range on a file.
    #[tracing::instrument(level = "trace", skip(self, query), fields(inode, owner = query.owner))]
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

        let plocks: Vec<EtcdPlock> = self
            .etcd_get_json_serde_only(&key)
            .await?
            .unwrap_or_default();

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
    #[tracing::instrument(
        level = "trace",
        skip(self),
        fields(inode, owner, block, lock_type = ?lock_type, pid)
    )]
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
mod tests;
