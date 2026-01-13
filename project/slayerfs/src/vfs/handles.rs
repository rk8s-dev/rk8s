//! File and directory handle management

use crate::meta::store::FileAttr;
use crate::vfs::fs::DirEntry;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;
use tokio::task::JoinHandle;

/// Maximum entries to return per readdir/readdirplus call
/// The setting should be based on the size of the cache handled by the kernel at one time.
pub const MAX_READDIR_ENTRIES: usize = 50;

#[allow(dead_code)]
pub struct FileHandle {
    pub fh: u64,
    pub ino: i64,
    pub attr: FileAttr,
    pub opened_at: Instant,
    pub last_offset: u64,
    pub flags: HandleFlags,
}

impl FileHandle {
    pub fn new(fh: u64, ino: i64, attr: FileAttr, flags: HandleFlags) -> Self {
        Self {
            fh,
            ino,
            attr,
            opened_at: Instant::now(),
            last_offset: 0,
            flags,
        }
    }
}

#[allow(dead_code)]
#[derive(Clone, Copy)]
pub struct HandleFlags {
    pub read: bool,
    pub write: bool,
}

impl HandleFlags {
    pub const fn new(read: bool, write: bool) -> Self {
        Self { read, write }
    }
}

/// Directory handle for caching directory listing during opendir-releasedir lifecycle
pub struct DirHandle {
    pub ino: i64,
    pub entries: Vec<DirEntry>,
    #[allow(dead_code)]
    pub opened_at: Instant,
    /// Background task handle for batch attribute prefetching
    pub prefetch_task: Option<JoinHandle<()>>,
    /// Flag indicating whether prefetch task has completed
    pub prefetch_done: Arc<AtomicBool>,
}

impl DirHandle {
    #[allow(unused)]
    pub fn new(ino: i64, entries: Vec<DirEntry>) -> Self {
        Self {
            ino,
            entries,
            opened_at: Instant::now(),
            prefetch_task: None,
            prefetch_done: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn with_prefetch_task(
        ino: i64,
        entries: Vec<DirEntry>,
        task: JoinHandle<()>,
        done_flag: Arc<AtomicBool>,
    ) -> Self {
        Self {
            ino,
            entries,
            opened_at: Instant::now(),
            prefetch_task: Some(task),
            prefetch_done: done_flag,
        }
    }

    /// Get entries starting from offset, limited to MAX_READDIR_ENTRIES
    pub fn get_entries(&self, offset: u64) -> Vec<DirEntry> {
        let start = offset as usize;
        if start >= self.entries.len() {
            return Vec::new();
        }
        let end = std::cmp::min(start + MAX_READDIR_ENTRIES, self.entries.len());
        self.entries[start..end].to_vec()
    }

    /// Get total number of entries
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if directory is empty
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Drop for DirHandle {
    fn drop(&mut self) {
        // Abort prefetch task if still running
        if let Some(task) = self.prefetch_task.take() {
            let is_done = self.prefetch_done.load(Ordering::Acquire);
            if !is_done {
                tracing::trace!("Aborting prefetch task for dir ino={}", self.ino);
                task.abort();
            }
        }
    }
}
