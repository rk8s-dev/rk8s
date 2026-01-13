//! Etcd backend-specific data structures

use crate::meta::Permission;
use crate::meta::entities::content_meta::EntryType;
use crate::meta::file_lock::PlockRecord;
use crate::meta::store::{FileAttr, FileType};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Etcd entry information (reverse index: inode -> file/directory attributes)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EtcdEntryInfo {
    pub is_file: bool,
    pub size: Option<i64>,
    pub version: Option<i32>,
    pub access_time: i64,
    pub modify_time: i64,
    pub create_time: i64,
    pub permission: Permission,
    pub nlink: u32,
    pub parent_inode: i64,
    pub entry_name: String,
    pub deleted: bool,
    #[serde(default)]
    pub symlink_target: Option<String>,
}

/// Etcd forward index entry ((parent_id, name) -> inode)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EtcdForwardEntry {
    pub parent_inode: i64,
    pub name: String,
    pub inode: i64,
    pub is_file: bool,
    #[serde(default)]
    pub entry_type: Option<EntryType>,
}

/// Etcd directory children collection (dir_id -> name to inode mapping)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EtcdDirChildren {
    pub inode: i64,
    pub children: HashMap<String, i64>,
}

impl EtcdDirChildren {
    /// Create new children collection with name->inode mapping
    pub fn new(inode: i64, children: HashMap<String, i64>) -> Self {
        Self { inode, children }
    }
}
#[allow(dead_code)]
impl EtcdEntryInfo {
    pub fn permission(&self) -> &Permission {
        &self.permission
    }

    pub fn set_permission(&mut self, permission: Permission) {
        self.permission = permission;
    }

    pub fn mode(&self) -> u32 {
        self.permission.mode
    }

    pub fn uid(&self) -> u32 {
        self.permission.uid
    }

    pub fn gid(&self) -> u32 {
        self.permission.gid
    }

    /// Converts EtcdEntryInfo to FileAttr for cache updates
    ///
    /// # Arguments
    ///
    /// * `ino` - The inode number (extracted from the r:{ino} key)
    ///
    /// # Returns
    ///
    /// FileAttr suitable for direct cache insertion
    pub fn to_file_attr(&self, ino: i64) -> FileAttr {
        let (kind, size) = if self.is_file {
            let file_type = if self.symlink_target.is_some() {
                FileType::Symlink
            } else {
                FileType::File
            };
            let file_size = if let Some(target) = &self.symlink_target {
                target.len() as u64
            } else {
                self.size.unwrap_or(0).max(0) as u64
            };
            (file_type, file_size)
        } else {
            (FileType::Dir, 4096)
        };

        FileAttr {
            ino,
            size,
            kind,
            mode: self.permission.mode,
            uid: self.permission.uid,
            gid: self.permission.gid,
            atime: self.access_time,
            mtime: self.modify_time,
            ctime: self.create_time,
            nlink: self.nlink,
        }
    }
}

impl EtcdForwardEntry {
    pub fn resolved_entry_type(&self) -> EntryType {
        if let Some(entry_type) = &self.entry_type {
            entry_type.clone()
        } else if self.is_file {
            EntryType::File
        } else {
            EntryType::Directory
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EtcdPlock {
    pub sid: Uuid,
    pub owner: i64,
    pub records: Vec<PlockRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EtcdLinkParent {
    pub parent_inode: i64,
    pub entry_name: String,
}
