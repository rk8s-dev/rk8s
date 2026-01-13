use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "link_parent_meta")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub inode: i64,

    #[sea_orm(primary_key, auto_increment = false)]
    pub parent_inode: i64,

    #[sea_orm(primary_key, auto_increment = false, column_type = "Text")]
    pub entry_name: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
