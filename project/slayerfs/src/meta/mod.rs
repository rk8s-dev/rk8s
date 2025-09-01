//! Metadata client and schema
//!
//! Responsibilities:
//! - Provide a transactional metadata client that talks to the chosen SQL
//!   backend (Postgres for production, SQLite for single-node development) via SQLx.
//! - Expose safe, atomic operations for inode/chunk/slice/block lifecycle updates.
//! - Maintain session registration and heartbeat records used for crash recovery
//!   and cleanup.
//!
//! Important notes / TODOs:
//! - Implement DB migrations and schema versioning.
//! - Ensure critical write-path updates (blocks + slice_blocks + slices + inode.size)
//!   are committed atomically.
//!
//! Submodules:
//! - `client`: transactional metadata client (SQLx wrappers)
//! - `migrations`: DB migration helpers
pub mod client;
pub mod migrations;

use crate::chuck::slice::SliceDesc;
use async_trait::async_trait;
use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::sync::{Arc, Mutex};

/// 错误类型（最小实现，后续可细化）
#[derive(Debug)]
pub enum MetaError {
    InodeNotFound(i64),
    Internal(String),
}

impl Display for MetaError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            MetaError::InodeNotFound(ino) => write!(f, "inode not found: {ino}"),
            MetaError::Internal(s) => write!(f, "internal error: {s}"),
        }
    }
}

impl std::error::Error for MetaError {}

/// Inode 元数据（最小集）
#[derive(Debug, Clone, Default)]
pub struct InodeMeta {
    pub ino: i64,
    pub size: u64,
    pub slices: Vec<SliceDesc>,
}

/// 元数据存储接口（异步）。
#[async_trait]
pub trait MetaStore: Send + Sync {
    async fn begin(&self) -> Box<dyn MetaTxn>;
    async fn get_inode_meta(&self, ino: i64) -> Option<InodeMeta>;
}

/// 事务接口：收集变更并一次性提交。
#[async_trait]
pub trait MetaTxn: Send {
    /// 立即分配并返回一个新的 inode 编号。
    async fn alloc_inode(&mut self) -> i64;
    async fn record_slice(&mut self, ino: i64, slice: SliceDesc) -> Result<(), MetaError>;
    async fn update_inode_size(&mut self, ino: i64, new_size: u64) -> Result<(), MetaError>;
    async fn commit(self: Box<Self>) -> Result<(), MetaError>;
    async fn rollback(self: Box<Self>);
}

// ================= In-memory 实现 =================

#[derive(Default, Clone)]
struct State {
    next_ino: i64,
    inodes: HashMap<i64, InodeMeta>,
}

pub struct InMemoryMetaStore {
    inner: Arc<Mutex<State>>,
}

impl InMemoryMetaStore {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(State::default())),
        }
    }
}

impl Default for InMemoryMetaStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl MetaStore for InMemoryMetaStore {
    async fn begin(&self) -> Box<dyn MetaTxn> {
        Box::new(InMemoryTxn {
            store: self.inner.clone(),
            staged: Vec::new(),
            created: Vec::new(),
        })
    }

    async fn get_inode_meta(&self, ino: i64) -> Option<InodeMeta> {
        self.inner
            .lock()
            .ok()
            .and_then(|g| g.inodes.get(&ino).cloned())
    }
}

enum Op {
    RecordSlice { ino: i64, slice: SliceDesc },
    UpdateSize { ino: i64, size: u64 },
}

struct InMemoryTxn {
    store: Arc<Mutex<State>>,
    staged: Vec<Op>,
    created: Vec<i64>,
}

#[async_trait]
impl MetaTxn for InMemoryTxn {
    async fn alloc_inode(&mut self) -> i64 {
        let mut guard = self.store.lock().expect("lock");
        guard.next_ino += 1;
        let ino = guard.next_ino;
        // 仅记录，提交时创建
        self.created.push(ino);
        ino
    }

    async fn record_slice(&mut self, ino: i64, slice: SliceDesc) -> Result<(), MetaError> {
        self.staged.push(Op::RecordSlice { ino, slice });
        Ok(())
    }

    async fn update_inode_size(&mut self, ino: i64, new_size: u64) -> Result<(), MetaError> {
        self.staged.push(Op::UpdateSize {
            ino,
            size: new_size,
        });
        Ok(())
    }

    async fn commit(self: Box<Self>) -> Result<(), MetaError> {
        let mut guard = self
            .store
            .lock()
            .map_err(|e| MetaError::Internal(format!("lock poisoned: {e}")))?;
        // 先创建本事务分配的 inode
        for ino in &self.created {
            guard.inodes.entry(*ino).or_insert_with(|| InodeMeta {
                ino: *ino,
                ..Default::default()
            });
        }
        // 应用变更
        for op in self.staged {
            match op {
                Op::RecordSlice { ino, slice } => {
                    let inode = guard.inodes.entry(ino).or_insert_with(|| InodeMeta {
                        ino,
                        ..Default::default()
                    });
                    inode.slices.push(slice);
                }
                Op::UpdateSize { ino, size } => {
                    let inode = guard.inodes.entry(ino).or_insert_with(|| InodeMeta {
                        ino,
                        ..Default::default()
                    });
                    // 允许扩展和收缩：直接赋值
                    inode.size = size;
                }
            }
        }
        Ok(())
    }

    async fn rollback(self: Box<Self>) {
        // 丢弃 staged 变更即可
    }
}

// ================= Tests =================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chuck::chunk::ChunkLayout;

    #[tokio::test]
    async fn test_meta_commit_and_size() {
        let meta = InMemoryMetaStore::new();
        let mut txn = meta.begin().await;

        // 模拟一次写入：inode 分配 + 记录 slice + 更新 size
        let ino = txn.alloc_inode().await;
        let layout = ChunkLayout::default();
        let slice = SliceDesc {
            slice_id: 0,
            chunk_id: 10,
            offset: 0,
            length: layout.block_size,
        };
        txn.record_slice(ino, slice).await.unwrap();
        txn.update_inode_size(ino, layout.block_size as u64)
            .await
            .unwrap();
        txn.commit().await.unwrap();

        // 开新事务读取（通过内部状态）
        let guard = meta.inner.lock().unwrap();
        let inode = guard.inodes.values().next().cloned().unwrap();
        assert_eq!(inode.size, layout.block_size as u64);
        assert_eq!(inode.slices.len(), 1);
    }

    #[tokio::test]
    async fn test_meta_rollback() {
        let meta = InMemoryMetaStore::new();
        let before_cnt = { meta.inner.lock().unwrap().inodes.len() };
        let mut txn = meta.begin().await;
        let ino = txn.alloc_inode().await;
        let slice = SliceDesc {
            slice_id: 0,
            chunk_id: 1,
            offset: 0,
            length: 1,
        };
        txn.record_slice(ino, slice).await.unwrap();
        txn.rollback().await; // 放弃
        let after_cnt = { meta.inner.lock().unwrap().inodes.len() };
        assert_eq!(before_cnt, after_cnt);
    }
}
