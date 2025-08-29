//! Chunk/slice 管理（chuck）
//!
//! 设计对标 JuiceFS：
//! - 文件被切分为固定大小的 Chunk（例如 64MiB），Chunk 内再划分为固定大小的 Block（例如 4MiB）。
//! - 写入时形成 Slice（对 Chunk 的一段连续范围），Slice 由若干 Block 片段组成；提交后通过元数据记录 Slice 与 Block 的映射。
//! - 读取时根据 (ino, chunk_index) 找到相关 Slice，按偏移做 Block 级拼接。
//!
//! 本模块提供：
//! - 常量与布局：默认 CHUNK/BLOCK 尺寸；偏移到 chunk/block 的映射函数。
//! - Slice 描述与 block 拆分工具。
//! - 轻量内存索引（仅用于演示/单机开发），后续可由 `meta` 层替代为持久化。
//!
//! 注意：当前仓库的其他模块仍是占位符；这里不做持久化与远端对象存储的交互，仅提供纯计算逻辑和基础数据结构。

#![allow(unused_imports)]

pub mod chunk;
pub mod reader;
pub mod slice;
pub mod store;
pub mod util;
pub mod writer;

pub use chunk::{
    ChunkLayout, DEFAULT_BLOCK_SIZE, DEFAULT_CHUNK_SIZE, chunk_index_of, within_chunk_offset,
};
pub use reader::ChunkReader;
pub use slice::{BlockSpan, SliceDesc};
pub use store::{BlockStore, InMemoryBlockStore, ObjectBlockStore, RustfsBlockStore, S3BlockStore};
pub use util::{ChunkSpan, split_file_range_into_chunks};
pub use writer::ChunkWriter;
