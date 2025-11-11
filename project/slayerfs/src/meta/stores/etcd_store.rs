//! Etcd-based metadata store implementation
//!
//! Uses Etcd/etcd as the backend for metadata storage

use crate::chuck::SliceDesc;
use crate::chuck::slice::key_for_slice;
use crate::meta::config::{Config, DatabaseType};
use crate::meta::entities::etcd::*;
use crate::meta::entities::*;
use crate::meta::store::{DirEntry, FileAttr, MetaError, MetaStore};
use crate::meta::{INODE_ID_KEY, Permission};
use crate::vfs::fs::FileType;
use async_trait::async_trait;
use chrono::Utc;
use etcd_client::{Client as EtcdClient, Compare, CompareOp, Txn, TxnOp};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json;
use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;
use tracing::{error, info, warn};

/// Etcd-based metadata store
pub struct EtcdMetaStore {
    client: EtcdClient,
    _config: Config,
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

    /// Create or open an etcd metadata store
    pub async fn new(backend_path: &Path) -> Result<Self, MetaError> {
        let _config =
            Config::from_path(backend_path).map_err(|e| MetaError::Config(e.to_string()))?;

        info!("Initializing EtcdMetaStore");
        info!("Backend path: {}", backend_path.display());

        let client = Self::create_client(&_config).await?;
        let store = Self { client, _config };
        store.init_root_directory().await?;

        info!("EtcdMetaStore initialized successfully");
        Ok(store)
    }

    /// Create from existing config
    pub async fn from_config(_config: Config) -> Result<Self, MetaError> {
        info!("Initializing EtcdMetaStore from config");

        let client = Self::create_client(&_config).await?;
        let store = Self { client, _config };
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
            DatabaseType::Sqlite { .. } | DatabaseType::Postgres { .. } => {
                Err(MetaError::Config(
                    "SQL database backend not supported by EtcdMetaStore. Use DatabaseMetaStore instead."
                        .to_string(),
                ))
            }
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
    ) -> Result<(), MetaError> {
        let mut client = self.client.clone();

        let json = serde_json::to_string(obj)?;
        let key = key.as_ref();

        client
            .put(key, json, None)
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
                let entry_type = if forward_entry.is_file {
                    EntryType::File
                } else {
                    EntryType::Directory
                };

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
                entry_info.deleted,
            );
            return Ok(Some(file_meta));
        }
        Ok(None)
    }

    /// Create a new directory
    async fn create_directory(&self, parent_inode: i64, name: String) -> Result<i64, MetaError> {
        // Step 1: Verify parent exists
        if self.get_access_meta(parent_inode).await?.is_none() {
            return Err(MetaError::ParentNotFound(parent_inode));
        }

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
        let dir_permission = Permission::new(0o40755, 0, 0);
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
        };

        let forward_key = Self::etcd_forward_key(parent_inode, &name);
        let forward_entry = EtcdForwardEntry {
            parent_inode,
            name: name.clone(),
            inode,
            is_file: false,
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
        // Step 1: Verify parent exists
        if self.get_access_meta(parent_inode).await?.is_none() {
            return Err(MetaError::ParentNotFound(parent_inode));
        }

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
        let file_permission = Permission::new(0o644, 0, 0);
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
        };

        let forward_key = Self::etcd_forward_key(parent_inode, &name);
        let forward_entry = EtcdForwardEntry {
            parent_inode,
            name: name.clone(),
            inode,
            is_file: true,
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

    /// Generate unique ID using Etcd atomic counter
    /// Uses compare-and-swap to ensure atomicity in distributed environment
    async fn generate_id(&self, counter_key: &str) -> Result<i64, MetaError> {
        let mut client = self.client.clone();

        // Retry loop for CAS operation
        // TODO: think about how to keep in sync with remote
        for retry in 0..10 {
            match client.get(counter_key, None).await {
                Ok(resp) => {
                    let (current_id, mod_revision) = if let Some(kv) = resp.kvs().first() {
                        let id = String::from_utf8_lossy(kv.value())
                            .parse::<i64>()
                            .unwrap_or(1);
                        (id, kv.mod_revision())
                    } else {
                        // First time initialization
                        if let Err(e) = client.put(counter_key, "2", None).await {
                            error!("Failed to initialize ID counter: {}", e);
                            return Err(MetaError::Config(format!(
                                "Failed to initialize ID counter: {}",
                                e
                            )));
                        }
                        return Ok(2);
                    };

                    let next_id = current_id + 1;

                    // Use transaction for atomic compare-and-swap

                    let cmp = Compare::mod_revision(counter_key, CompareOp::Equal, mod_revision);
                    let put_op = TxnOp::put(counter_key, next_id.to_string(), None);
                    let txn = Txn::new().when([cmp]).and_then([put_op]);

                    match client.txn(txn).await {
                        Ok(txn_resp) => {
                            if txn_resp.succeeded() {
                                // CAS succeeded, return the new ID
                                return Ok(next_id);
                            } else {
                                // CAS failed, retry
                                if retry < 9 {
                                    continue;
                                } else {
                                    return Err(MetaError::Config(
                                        "Failed to generate ID after max retries".to_string(),
                                    ));
                                }
                            }
                        }
                        Err(e) => {
                            error!("Failed to execute transaction: {}", e);
                            return Err(MetaError::Config(format!(
                                "Failed to execute ID generation transaction: {}",
                                e
                            )));
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to get ID counter: {}", e);
                    return Err(MetaError::Config(format!(
                        "Failed to get ID counter: {}",
                        e
                    )));
                }
            }
        }

        Err(MetaError::Config(
            "Failed to generate ID: max retries exceeded".to_string(),
        ))
    }

    async fn atomic_update<F, T>(&self, key: &str, f: F, init: T) -> Result<(), MetaError>
    where
        F: Fn(T) -> T,
        T: Serialize + DeserializeOwned + Clone,
    {
        let mut client = self.client.clone();

        for _ in 0..10 {
            let resp = client
                .get(key, None)
                .await
                .map_err(|e| MetaError::Config(format!("Failed to get key: {e}")))?;

            let (current, version) = match resp.kvs().first() {
                Some(kv) => (f(serde_json::from_slice::<T>(kv.value())?), kv.version()),
                None => (init.clone(), 0),
            };
            let current = serde_json::to_string(&current)?;

            let compare = Compare::version(key, CompareOp::Equal, version);
            let op = TxnOp::put(key, current, None);
            let txn = Txn::new().when([compare]).and_then([op]);

            match client.txn(txn).await {
                Ok(txn_resp) if txn_resp.succeeded() => return Ok(()),
                Ok(_) => {}
                Err(e) => {
                    return Err(MetaError::Config(format!(
                        "Failed to execute transaction: {e}"
                    )));
                }
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        Err(MetaError::MaxRetriesExceeded)
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
        let mut client = self.client.clone();

        for retry in 0..max_retries {
            let resp = client.get(key.clone(), None).await.map_err(|e| {
                MetaError::Internal(format!("Failed to get parent children: {}", e))
            })?;

            let (mut children_map, mod_revision) = if let Some(kv) = resp.kvs().first() {
                let children: EtcdDirChildren = serde_json::from_slice(kv.value())
                    .map_err(|e| MetaError::Internal(format!("Failed to parse children: {}", e)))?;
                (children.children.clone(), kv.mod_revision())
            } else {
                (HashMap::new(), 0)
            };

            updater(&mut children_map);

            let new_children = EtcdDirChildren::new(parent_ino, children_map);
            let new_json = serde_json::to_string(&new_children)
                .map_err(|e| MetaError::Internal(format!("Failed to serialize children: {}", e)))?;

            let txn = if mod_revision == 0 {
                Txn::new()
                    .when([Compare::version(key.clone(), CompareOp::Equal, 0)])
                    .and_then([TxnOp::put(key.clone(), new_json, None)])
            } else {
                Txn::new()
                    .when([Compare::mod_revision(
                        key.clone(),
                        CompareOp::Equal,
                        mod_revision,
                    )])
                    .and_then([TxnOp::put(key.clone(), new_json, None)])
            };

            let txn_resp = client.txn(txn).await.map_err(|e| {
                MetaError::Internal(format!("Update children transaction failed: {}", e))
            })?;

            if txn_resp.succeeded() {
                return Ok(());
            } else if retry < max_retries - 1 {
                info!(
                    "Concurrent update detected for parent {}, retrying ({}/{})",
                    parent_ino,
                    retry + 1,
                    max_retries
                );
                continue;
            } else {
                return Err(MetaError::Internal(format!(
                    "Failed to update parent {} children after {} retries",
                    parent_ino, max_retries
                )));
            }
        }
        Err(MetaError::Internal(
            "Update parent children failed: unreachable".to_string(),
        ))
    }
}

#[async_trait]
impl MetaStore for EtcdMetaStore {
    async fn stat(&self, ino: i64) -> Result<Option<FileAttr>, MetaError> {
        if let Ok(Some(file_meta)) = self.get_file_meta(ino).await {
            let permission = file_meta.permission();
            return Ok(Some(FileAttr {
                ino: file_meta.inode,
                size: file_meta.size as u64,
                kind: FileType::File,
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

        // Update file metadata - mark as deleted or decrease nlink
        if entry_info.nlink > 1 {
            // File has multiple links, just decrease nlink
            entry_info.nlink -= 1;
        } else {
            // Last link, mark as deleted
            entry_info.deleted = true;
            entry_info.nlink = 0;
        }

        entry_info.modify_time = Utc::now().timestamp_nanos_opt().unwrap_or(0);

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

        info!(
            "Rename completed successfully: {} -> {}, inode={}",
            old_name, new_name, entry_ino
        );

        Ok(())
    }

    async fn set_file_size(&self, ino: i64, size: u64) -> Result<(), MetaError> {
        let reverse_key = Self::etcd_reverse_key(ino);
        let mut client = self.client.clone();

        // Use optimistic locking to prevent lost updates
        for retry in 0..10 {
            let resp = client
                .get(reverse_key.clone(), None)
                .await
                .map_err(|e| MetaError::Internal(format!("Failed to get file metadata: {}", e)))?;

            let (mut entry_info, mod_revision) = if let Some(kv) = resp.kvs().first() {
                let info: EtcdEntryInfo = serde_json::from_slice(kv.value())
                    .map_err(|e| MetaError::Internal(format!("Failed to parse metadata: {}", e)))?;
                (info, kv.mod_revision())
            } else {
                return Err(MetaError::NotFound(ino));
            };

            if !entry_info.is_file {
                return Err(MetaError::Internal(
                    "Cannot set size for directory".to_string(),
                ));
            }

            entry_info.size = Some(size as i64);
            entry_info.modify_time = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0);

            let updated_json = serde_json::to_string(&entry_info).map_err(|e| {
                MetaError::Internal(format!("Failed to serialize entry info: {}", e))
            })?;

            // Use CAS to ensure atomicity
            let txn = Txn::new()
                .when([Compare::mod_revision(
                    reverse_key.clone(),
                    CompareOp::Equal,
                    mod_revision,
                )])
                .and_then([TxnOp::put(reverse_key.clone(), updated_json, None)]);

            let txn_resp = client
                .txn(txn)
                .await
                .map_err(|e| MetaError::Internal(format!("Failed to update file size: {}", e)))?;

            if txn_resp.succeeded() {
                return Ok(());
            } else if retry < 9 {
                info!(
                    "Concurrent update detected for inode {}, retrying ({}/10)",
                    ino,
                    retry + 1
                );
                tokio::time::sleep(Duration::from_millis(50)).await;
                continue;
            } else {
                return Err(MetaError::Internal(format!(
                    "Failed to update file size after 10 retries (inode: {})",
                    ino
                )));
            }
        }

        Err(MetaError::Internal(
            "set_file_size: unreachable".to_string(),
        ))
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
        self.atomic_update(
            &key,
            |mut source: Vec<SliceDesc>| {
                source.push(slice);
                source
            },
            vec![slice],
        )
        .await
    }

    async fn next_id(&self, key: &str) -> Result<i64, MetaError> {
        self.generate_id(key).await
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
