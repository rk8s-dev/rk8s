use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "xattr")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub inode: i64,
    #[sea_orm(primary_key)]
    pub name: String,
    pub value: Vec<u8>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
