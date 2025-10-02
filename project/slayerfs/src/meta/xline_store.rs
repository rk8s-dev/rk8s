//! Etcd-based metadata store implementation
//!
//! Uses Xline/etcd as the backend for metadata storage

use crate::meta::Permission;
use crate::meta::config::{Config, DatabaseType};
use crate::meta::entities::xline::*;
use crate::meta::entities::*;
use crate::meta::store::{DirEntry, FileAttr, MetaError, MetaStore};
use crate::vfs::fs::FileType;
use async_trait::async_trait;
use chrono::Utc;
use etcd_client::Client as XlineClient;
use log::{error, info};
use serde_json;
use std::collections::HashSet;
use std::path::Path;

/// Etcd-based metadata store
pub struct XlineMetaStore {
    client: XlineClient,
    _config: Config,
}
#[allow(dead_code)]
impl XlineMetaStore {
    /// Xline helper method: generate forward index key (parent_inode, name)
    fn xline_forward_key(parent_inode: i64, name: &str) -> String {
        format!("f:{}:{}", parent_inode, name)
    }

    /// Xline helper method: generate reverse index key for inode
    fn xline_reverse_key(ino: i64) -> String {
        format!("r:{}", ino)
    }

    /// Xline helper method: generate directory children key
    fn xline_children_key(inode: i64) -> String {
        format!("c:{}", inode)
    }

    /// Create or open an etcd metadata store
    pub async fn new(backend_path: &Path) -> Result<Self, MetaError> {
        let _config =
            Config::from_path(backend_path).map_err(|e| MetaError::Config(e.to_string()))?;

        info!("Initializing XlineMetaStore");
        info!("Backend path: {}", backend_path.display());

        let client = Self::create_client(&_config).await?;
        let store = Self { client, _config };
        store.init_root_directory().await?;

        info!("XlineMetaStore initialized successfully");
        Ok(store)
    }

    /// Create from existing config
    pub async fn from_config(_config: Config) -> Result<Self, MetaError> {
        info!("Initializing XlineMetaStore from config");

        let client = Self::create_client(&_config).await?;
        let store = Self { client, _config };
        store.init_root_directory().await?;

        info!("XlineMetaStore initialized successfully");
        Ok(store)
    }

    /// Create etcd client
    async fn create_client(config: &Config) -> Result<XlineClient, MetaError> {
        match &config.database.db_config {
            DatabaseType::Xline { urls } => {
                info!("Connecting to Xline cluster: {:?}", urls);
                let client = XlineClient::connect(urls, None)
                    .await
                    .map_err(|e| MetaError::Config(format!("Failed to connect to Xline: {}", e)))?;
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
        let children_key = Self::xline_children_key(1);
        let mut client = self.client.clone();

        if let Ok(resp) = client.get(children_key.clone(), None).await
            && !resp.kvs().is_empty()
        {
            info!("Root directory already initialized for Etcd backend");
            return Ok(());
        }

        let root_children = XlineDirChildren {
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

        let reverse_key = Self::xline_reverse_key(inode);
        let mut client = self.client.clone();

        match client.get(reverse_key, None).await {
            Ok(resp) => {
                if let Some(kv) = resp.kvs().first() {
                    let entry_info: XlineEntryInfo = serde_json::from_slice(kv.value())?;
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
        let children_key = Self::xline_children_key(parent_inode);
        let mut client = self.client.clone();

        match client.get(children_key, None).await {
            Ok(resp) => {
                if let Some(kv) = resp.kvs().first() {
                    let dir_children: XlineDirChildren = serde_json::from_slice(kv.value())?;

                    if dir_children.children.is_empty() {
                        return Ok(None);
                    }

                    let mut content_list = Vec::new();

                    for child_name in &dir_children.children {
                        let forward_key = Self::xline_forward_key(parent_inode, child_name);
                        if let Ok(forward_resp) = client.get(forward_key, None).await
                            && let Some(forward_kv) = forward_resp.kvs().first()
                        {
                            let forward_entry: XlineForwardEntry =
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
        let reverse_key = Self::xline_reverse_key(inode);
        let mut client = self.client.clone();

        match client.get(reverse_key.clone(), None).await {
            Ok(resp) => {
                if let Some(kv) = resp.kvs().first() {
                    let entry_info: XlineEntryInfo = serde_json::from_slice(kv.value())?;

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
        let entry_info = XlineEntryInfo {
            is_file: false,
            size: None,
            version: None,
            permission: dir_permission,
            access_time: now,
            modify_time: now,
            create_time: now,
            nlink: 2,
        };

        let forward_key = Self::xline_forward_key(parent_inode, &name);
        let forward_entry = XlineForwardEntry {
            parent_inode,
            name: name.clone(),
            inode,
            is_file: false,
        };
        let forward_json = serde_json::to_string(&forward_entry)?;

        let reverse_key = Self::xline_reverse_key(inode);
        let reverse_json = serde_json::to_string(&entry_info)?;

        let children_key = Self::xline_children_key(inode);
        let children = XlineDirChildren {
            inode,
            children: HashSet::new(),
        };
        let children_json = serde_json::to_string(&children)?;

        let parent_children_key = Self::xline_children_key(parent_inode);
        let parent_children = match client.get(parent_children_key.clone(), None).await {
            Ok(resp) => {
                if let Some(kv) = resp.kvs().first() {
                    let mut children: XlineDirChildren = serde_json::from_slice(kv.value())?;
                    children.children.insert(name.clone());
                    children
                } else {
                    let mut children = XlineDirChildren {
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
        let entry_info = XlineEntryInfo {
            is_file: true,
            size: Some(0),
            version: Some(0),
            permission: file_permission,
            access_time: now,
            modify_time: now,
            create_time: now,
            nlink: 1,
        };

        let forward_key = Self::xline_forward_key(parent_inode, &name);
        let forward_entry = XlineForwardEntry {
            parent_inode,
            name: name.clone(),
            inode,
            is_file: true,
        };
        let forward_json = serde_json::to_string(&forward_entry)?;

        let reverse_key = Self::xline_reverse_key(inode);
        let reverse_json = serde_json::to_string(&entry_info)?;

        let parent_children_key = Self::xline_children_key(parent_inode);
        let parent_children = match client.get(parent_children_key.clone(), None).await {
            Ok(resp) => {
                if let Some(kv) = resp.kvs().first() {
                    let mut children: XlineDirChildren = serde_json::from_slice(kv.value())?;
                    children.children.insert(name.clone());
                    children
                } else {
                    let mut children = XlineDirChildren {
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

    /// Generate unique ID
    async fn generate_id(&self) -> Result<i64, MetaError> {
        let mut client = self.client.clone();
        let counter_key = "slayerfs:next_inode_id";

        match client.get(counter_key, None).await {
            Ok(resp) => {
                let current_id = if let Some(kv) = resp.kvs().first() {
                    String::from_utf8_lossy(kv.value())
                        .parse::<i64>()
                        .unwrap_or(1)
                } else {
                    1
                };

                let next_id = current_id + 1;

                if let Err(e) = client.put(counter_key, next_id.to_string(), None).await {
                    return Err(MetaError::Config(format!(
                        "Failed to update ID counter: {}",
                        e
                    )));
                }

                Ok(next_id)
            }
            Err(e) => Err(MetaError::Config(format!(
                "Failed to get ID counter: {}",
                e
            ))),
        }
    }
}

#[async_trait]
impl MetaStore for XlineMetaStore {
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
        let reverse_key = Self::xline_reverse_key(ino);

        match client.get(reverse_key.clone(), None).await {
            Ok(resp) => {
                if let Some(kv) = resp.kvs().first() {
                    let mut entry_info: XlineEntryInfo = serde_json::from_slice(kv.value())
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
