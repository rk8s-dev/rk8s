use crate::chuck::SliceDesc;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "slice_meta")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = true)]
    pub id: i64,
    pub slice_id: i64,
    pub chunk_id: i64,
    pub offset: i64,
    pub length: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

impl From<Model> for SliceDesc {
    fn from(model: Model) -> Self {
        Self {
            slice_id: model.slice_id as u64,
            chunk_id: model.chunk_id as u64,
            offset: model.offset as u64,
            length: model.length as u64,
        }
    }
}
