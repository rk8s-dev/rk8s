use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// Directory entry type enumeration
#[derive(Debug, Clone, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "i32", db_type = "Integer")]
pub enum EntryType {
    #[sea_orm(num_value = 0)]
    File,

    #[sea_orm(num_value = 1)]
    Directory,
}

/// Content metadata model
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "content_meta")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub inode: i64,

    #[sea_orm(indexed)]
    pub parent_inode: i64,

    pub entry_name: String,

    pub entry_type: EntryType,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
