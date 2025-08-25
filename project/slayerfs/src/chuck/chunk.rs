//! Chunk index and metadata helpers
//!
//! This module contains utilities to manage chunk-level metadata such as
//! chunk indexing, timestamps and quick lookup helpers.
#[allow(unused)]
pub struct ChunkMeta {
    pub chunk_id: i64,
    pub ino: i64,
    pub index: i32,
}

#[allow(unused)]
impl ChunkMeta {
    pub fn new(chunk_id: i64, ino: i64, index: i32) -> Self {
        Self {
            chunk_id,
            ino,
            index,
        }
    }
}
