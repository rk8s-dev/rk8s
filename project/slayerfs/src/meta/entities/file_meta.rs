use crate::meta::Permission;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "file_meta")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub inode: i64,

    pub size: i64,
    pub access_time: i64,
    pub modify_time: i64,
    pub create_time: i64,

    #[sea_orm(column_type = "Text")]
    pub permission: Permission,

    #[sea_orm(column_type = "Integer")]
    pub nlink: i32,

    /// Parent directory inode for single-link files.
    ///
    /// Behavior:
    /// - When nlink=1: Contains the parent directory inode (O(1) lookup optimization)
    /// - When nlink>1: Set to 0 and LinkParentMeta is used to track all parents
    #[sea_orm(column_type = "BigInteger", default_value = "0")]
    pub parent: i64,

    /// Whether the file is marked for deletion (for garbage collection)
    #[sea_orm(column_type = "Boolean", default_value = "false")]
    pub deleted: bool,

    /// Optional symbolic link target when this inode represents a symlink
    #[sea_orm(column_type = "Text", nullable)]
    pub symlink_target: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
#[allow(dead_code)]
impl Model {
    pub fn permission(&self) -> &Permission {
        &self.permission
    }

    #[allow(clippy::too_many_arguments)]
    pub fn from_permission(
        inode: i64,
        size: i64,
        permission: Permission,
        access_time: i64,
        modify_time: i64,
        create_time: i64,
        nlink: i32,
        parent: i64,
        deleted: bool,
        symlink_target: Option<String>,
    ) -> Self {
        Self {
            inode,
            size,
            permission,
            access_time,
            modify_time,
            create_time,
            nlink,
            parent,
            deleted,
            symlink_target,
        }
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
