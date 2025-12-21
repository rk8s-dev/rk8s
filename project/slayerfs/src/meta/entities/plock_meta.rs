use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "plock")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub inode: i64,
    #[sea_orm(primary_key)]
    pub sid: Uuid,
    #[sea_orm(primary_key)]
    pub owner: i64,
    pub records: Vec<u8>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}
