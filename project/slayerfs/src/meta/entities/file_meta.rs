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
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
#[allow(dead_code)]
impl Model {
    pub fn permission(&self) -> &Permission {
        &self.permission
    }

    pub fn from_permission(
        inode: i64,
        size: i64,
        permission: Permission,
        access_time: i64,
        modify_time: i64,
        create_time: i64,
        nlink: i32,
    ) -> Self {
        Self {
            inode,
            size,
            permission,
            access_time,
            modify_time,
            create_time,
            nlink,
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
