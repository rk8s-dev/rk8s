//! Slice lifecycle and mapping to blocks
//!
//! This module defines how slices map to blocks and contains helpers for
//! constructing slice->block mappings during the write path.

#[allow(unused)]
pub struct SliceDesc {
    pub slice_id: i64,
    pub chunk_id: i64,
    pub offset: i32,
    pub length: i32,
}

impl SliceDesc {
    #[allow(dead_code)]
    pub fn is_committed(&self) -> bool {
        // placeholder
        true
    }
}
