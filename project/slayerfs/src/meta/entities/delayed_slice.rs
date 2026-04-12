//! Delayed slice entity for soft deletion
//!
//! Records slices that are marked for deletion but not yet cleaned up.
//! This ensures data safety by allowing verification before permanent deletion.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "delayed_slice")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = true)]
    pub id: i64,
    /// Slice ID to be deleted
    pub slice_id: i64,
    /// Chunk ID that this slice belongs to
    pub chunk_id: i64,
    /// Offset within the chunk.
    ///
    /// Kept for compaction provenance/debug context. Current block-store GC deletion
    /// uses slice-relative block addressing and does not derive delete start index
    /// from this field.
    pub offset: i64,
    /// Size of the slice (for object storage cleanup)
    pub size: i64,
    /// Timestamp when this record was created
    pub created_at: i64,
    /// Deletion reason (e.g., "compact", "gc", "manual")
    pub reason: String,
    /// Deletion status: "pending", "meta_deleted", "confirmed"
    /// - pending: initial state, slice_meta still exists
    /// - meta_deleted: slice_meta deleted, waiting for block deletion
    /// - confirmed: block deleted, can be removed
    pub status: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
