//! Metadata Store Implementations
//!
//! This module contains all concrete implementations of the MetaStore trait.
//! Each store provides a different backend for metadata persistence:
//!
//! - `DatabaseMetaStore`: SQL databases (PostgreSQL, SQLite)
//! - `EtcdMetaStore`: Distributed etcd cluster
pub mod database_store;
pub mod etcd_store;
pub(crate) mod etcd_watch;
pub(crate) mod pool;
pub mod redis_store;

// Re-export main types for convenience
pub use database_store::DatabaseMetaStore;
pub use etcd_store::EtcdMetaStore;
pub(crate) use etcd_watch::{CacheInvalidationEvent, EtcdWatchWorker, WatchConfig};
pub use redis_store::RedisMetaStore;

struct TruncatePlan {
    cutoff_chunk: u64,
    cutoff_offset: u32,
    drop_start: u64,
    old_chunk_count: u64,
}

enum TrimAction {
    Keep,
    Drop,
    Truncate(u32),
}

fn truncate_plan(new_size: u64, old_size: u64, chunk_size: u64) -> Option<TruncatePlan> {
    if new_size >= old_size || chunk_size == 0 {
        return None;
    }

    let cutoff_chunk = new_size / chunk_size;
    let cutoff_offset = (new_size % chunk_size) as u32;
    let old_chunk_count = old_size / chunk_size + u64::from(!old_size.is_multiple_of(chunk_size));
    let drop_start = if cutoff_offset == 0 {
        cutoff_chunk
    } else {
        cutoff_chunk + 1
    };

    Some(TruncatePlan {
        cutoff_chunk,
        cutoff_offset,
        drop_start,
        old_chunk_count,
    })
}

fn trim_action(offset: u32, length: u32, cutoff_offset: u32) -> TrimAction {
    if offset >= cutoff_offset {
        return TrimAction::Drop;
    }

    let end = offset.saturating_add(length);
    if end > cutoff_offset {
        TrimAction::Truncate(cutoff_offset - offset)
    } else {
        TrimAction::Keep
    }
}

fn trim_slices_in_place(slices: &mut Vec<crate::chuck::SliceDesc>, cutoff_offset: u32) {
    slices.retain(|s| s.offset < cutoff_offset);
    for slice in slices.iter_mut() {
        let end = slice.offset + slice.length;
        if end > cutoff_offset {
            slice.length = cutoff_offset - slice.offset;
        }
    }
}

pub(crate) async fn apply_truncate_plan<E, R, D, FR, FD>(
    new_size: u64,
    old_size: u64,
    chunk_size: u64,
    mut rewrite: R,
    mut delete: D,
) -> Result<(), E>
where
    R: FnMut(u64, u32) -> FR,
    FR: std::future::Future<Output = Result<(), E>>,
    D: FnMut(u64, u64) -> FD,
    FD: std::future::Future<Output = Result<(), E>>,
{
    let Some(plan) = truncate_plan(new_size, old_size, chunk_size) else {
        return Ok(());
    };

    if plan.cutoff_offset > 0 {
        rewrite(plan.cutoff_chunk, plan.cutoff_offset).await?;
    }

    if plan.drop_start < plan.old_chunk_count {
        delete(plan.drop_start, plan.old_chunk_count).await?;
    }

    Ok(())
}
