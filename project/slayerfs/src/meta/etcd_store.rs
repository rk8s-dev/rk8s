//! Etcd-based metadata store implementation
//!
//! Uses Etcd/etcd as the backend for metadata storage

use crate::meta::Permission;
use crate::meta::config::{Config, DatabaseType};
use crate::meta::entities::etcd::*;
use crate::meta::entities::*;
use crate::meta::store::{DirEntry, FileAttr, MetaError, MetaStore};
use crate::vfs::fs::FileType;
use async_trait::async_trait;
use chrono::Utc;
use etcd_client::Client as EtcdClient;
use log::{error, info};
use serde_json;
use std::collections::HashSet;
use std::path::Path;

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

    /// Initialize root directory
    async fn init_root_directory(&self) -> Result<(), MetaError> {
        let children_key = Self::etcd_children_key(1);
        let mut client = self.client.clone();

        if let Ok(resp) = client.get(children_key.clone(), None).await
            && !resp.kvs().is_empty()
        {
            info!("Root directory already initialized for Etcd backend");
            return Ok(());
        }

        let root_children = EtcdDirChildren {
            inode: 1,
            children: HashSet::new(),
        };

        let children_json = serde_json::to_string(&root_children)?;
        client
            .put(children_key, children_json, None)
            .await
            .map_err(|e| {
                MetaError::Config(format!(
                    "Failed to initialize root directory in Etcd: {}",
                    e
                ))
            })?;

        info!("Root directory initialized for Etcd backend");
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
        let mut client = self.client.clone();

        match client.get(reverse_key, None).await {
            Ok(resp) => {
                if let Some(kv) = resp.kvs().first() {
                    let entry_info: EtcdEntryInfo = serde_json::from_slice(kv.value())?;
                    if !entry_info.is_file {
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
                }
                Ok(None)
            }
            Err(_) => Ok(None),
        }
    }

    /// Get directory content metadata
    async fn get_content_meta(
        &self,
        parent_inode: i64,
    ) -> Result<Option<Vec<ContentMetaModel>>, MetaError> {
        let children_key = Self::etcd_children_key(parent_inode);
        let mut client = self.client.clone();

        match client.get(children_key, None).await {
            Ok(resp) => {
                if let Some(kv) = resp.kvs().first() {
                    let dir_children: EtcdDirChildren = serde_json::from_slice(kv.value())?;

                    if dir_children.children.is_empty() {
                        return Ok(None);
                    }

                    let mut content_list = Vec::new();

                    for child_name in &dir_children.children {
                        let forward_key = Self::etcd_forward_key(parent_inode, child_name);
                        if let Ok(forward_resp) = client.get(forward_key, None).await
                            && let Some(forward_kv) = forward_resp.kvs().first()
                        {
                            let forward_entry: EtcdForwardEntry =
                                serde_json::from_slice(forward_kv.value())?;

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
                } else {
                    Ok(None)
                }
            }
            Err(_) => Ok(None),
        }
    }

    /// Get file metadata
    async fn get_file_meta(&self, inode: i64) -> Result<Option<FileMetaModel>, MetaError> {
        let reverse_key = Self::etcd_reverse_key(inode);
        let mut client = self.client.clone();

        match client.get(reverse_key.clone(), None).await {
            Ok(resp) => {
                if let Some(kv) = resp.kvs().first() {
                    let entry_info: EtcdEntryInfo = serde_json::from_slice(kv.value())?;

                    if entry_info.is_file {
                        let permission = entry_info.permission().clone();
                        let file_meta = FileMetaModel::from_permission(
                            inode,
                            entry_info.size.unwrap_or(0),
                            permission,
                            entry_info.access_time,
                            entry_info.modify_time,
                            entry_info.create_time,
                            entry_info.nlink as i32,
                        );
                        Ok(Some(file_meta))
                    } else {
                        Ok(None)
                    }
                } else {
                    Ok(None)
                }
            }
            Err(e) => {
                error!("Failed to get file meta from Etcd: {}", e);
                Ok(None)
            }
        }
    }

    /// Create a new directory
    async fn create_directory(&self, parent_inode: i64, name: String) -> Result<i64, MetaError> {
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

        let inode = self.generate_id().await?;
        let mut client = self.client.clone();

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
        let children = EtcdDirChildren {
            inode,
            children: HashSet::new(),
        };
        let children_json = serde_json::to_string(&children)?;

        let parent_children_key = Self::etcd_children_key(parent_inode);
        let parent_children = match client.get(parent_children_key.clone(), None).await {
            Ok(resp) => {
                if let Some(kv) = resp.kvs().first() {
                    let mut children: EtcdDirChildren = serde_json::from_slice(kv.value())?;
                    children.children.insert(name.clone());
                    children
                } else {
                    let mut children = EtcdDirChildren {
                        inode: parent_inode,
                        children: HashSet::new(),
                    };
                    children.children.insert(name.clone());
                    children
                }
            }
            Err(e) => {
                error!("Failed to get parent directory children: {}", e);
                return Err(MetaError::Config(format!(
                    "Failed to get parent directory children: {}",
                    e
                )));
            }
        };
        let parent_children_json = serde_json::to_string(&parent_children)?;

        client
            .put(forward_key, forward_json, None)
            .await
            .map_err(|e| MetaError::Config(format!("Failed to create forward index: {}", e)))?;
        client
            .put(reverse_key, reverse_json, None)
            .await
            .map_err(|e| MetaError::Config(format!("Failed to create reverse index: {}", e)))?;
        client
            .put(children_key, children_json, None)
            .await
            .map_err(|e| MetaError::Config(format!("Failed to create children index: {}", e)))?;
        client
            .put(parent_children_key, parent_children_json, None)
            .await
            .map_err(|e| MetaError::Config(format!("Failed to update parent children: {}", e)))?;

        Ok(inode)
    }

    /// Create a new file
    async fn create_file_internal(
        &self,
        parent_inode: i64,
        name: String,
    ) -> Result<i64, MetaError> {
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

        let inode = self.generate_id().await?;
        let mut client = self.client.clone();

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

        let parent_children_key = Self::etcd_children_key(parent_inode);
        let parent_children = match client.get(parent_children_key.clone(), None).await {
            Ok(resp) => {
                if let Some(kv) = resp.kvs().first() {
                    let mut children: EtcdDirChildren = serde_json::from_slice(kv.value())?;
                    children.children.insert(name.clone());
                    children
                } else {
                    let mut children = EtcdDirChildren {
                        inode: parent_inode,
                        children: HashSet::new(),
                    };
                    children.children.insert(name.clone());
                    children
                }
            }
            Err(e) => {
                error!("Failed to get parent directory children: {}", e);
                return Err(MetaError::Config(format!(
                    "Failed to get parent directory children: {}",
                    e
                )));
            }
        };
        let parent_children_json = serde_json::to_string(&parent_children)?;

        client
            .put(forward_key, forward_json, None)
            .await
            .map_err(|e| MetaError::Config(format!("Failed to create forward index: {}", e)))?;
        client
            .put(reverse_key, reverse_json, None)
            .await
            .map_err(|e| MetaError::Config(format!("Failed to create reverse index: {}", e)))?;
        client
            .put(parent_children_key, parent_children_json, None)
            .await
            .map_err(|e| MetaError::Config(format!("Failed to update parent children: {}", e)))?;

        Ok(inode)
    }

    /// Generate unique ID using Etcd atomic counter
    /// Uses compare-and-swap to ensure atomicity in distributed environment
    async fn generate_id(&self) -> Result<i64, MetaError> {
        let mut client = self.client.clone();
        let counter_key = "slayerfs:next_inode_id";

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
                    use etcd_client::{Compare, CompareOp, Txn, TxnOp};

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
        let contents = match self.get_content_meta(parent).await? {
            Some(contents) => contents,
            None => return Ok(None),
        };

        for content in contents {
            if content.entry_name == name {
                return Ok(Some(content.inode));
            }
        }

        Ok(None)
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

    async fn rmdir(&self, _parent: i64, _name: &str) -> Result<(), MetaError> {
        Err(MetaError::Internal(
            "rmdir not implemented for Etcd".to_string(),
        ))
    }

    async fn create_file(&self, parent: i64, name: String) -> Result<i64, MetaError> {
        self.create_file_internal(parent, name).await
    }

    async fn unlink(&self, _parent: i64, _name: &str) -> Result<(), MetaError> {
        Err(MetaError::Internal(
            "unlink not implemented for Etcd".to_string(),
        ))
    }

    async fn rename(
        &self,
        _old_parent: i64,
        _old_name: &str,
        _new_parent: i64,
        _new_name: String,
    ) -> Result<(), MetaError> {
        Err(MetaError::Internal(
            "rename not implemented for Etcd".to_string(),
        ))
    }

    async fn set_file_size(&self, ino: i64, size: u64) -> Result<(), MetaError> {
        let mut client = self.client.clone();
        let reverse_key = Self::etcd_reverse_key(ino);

        match client.get(reverse_key.clone(), None).await {
            Ok(resp) => {
                if let Some(kv) = resp.kvs().first() {
                    let mut entry_info: EtcdEntryInfo = serde_json::from_slice(kv.value())
                        .map_err(|e| {
                            MetaError::Internal(format!("Failed to parse entry info: {}", e))
                        })?;

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

                    client
                        .put(reverse_key, updated_json, None)
                        .await
                        .map_err(|e| {
                            MetaError::Internal(format!(
                                "Failed to update file size in Etcd: {}",
                                e
                            ))
                        })?;

                    Ok(())
                } else {
                    Err(MetaError::NotFound(ino))
                }
            }
            Err(e) => Err(MetaError::Internal(format!(
                "Failed to get file from Etcd: {}",
                e
            ))),
        }
    }

    fn root_ino(&self) -> i64 {
        1
    }

    async fn initialize(&self) -> Result<(), MetaError> {
        Ok(())
    }
}
