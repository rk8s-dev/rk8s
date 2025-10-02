//! Xline backend-specific data structures

use crate::meta::Permission;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Xline entry information (reverse index: inode -> file/directory attributes)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XlineEntryInfo {
    pub is_file: bool,
    pub size: Option<i64>,
    pub version: Option<i32>,
    pub access_time: i64,
    pub modify_time: i64,
    pub create_time: i64,
    pub permission: Permission,
    pub nlink: u32,
}

/// Xline forward index entry ((parent_id, name) -> inode)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XlineForwardEntry {
    pub parent_inode: i64,
    pub name: String,
    pub inode: i64,
    pub is_file: bool,
}

/// Xline directory children collection (dir_id -> children names)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XlineDirChildren {
    pub inode: i64,
    pub children: HashSet<String>,
}
#[allow(dead_code)]
impl XlineEntryInfo {
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
}
