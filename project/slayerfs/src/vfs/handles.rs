//! File and directory handle management

use crate::vfs::fs::DirEntry;
use std::time::Instant;

/// Maximum entries to return per readdir/readdirplus call
/// The setting should be based on the size of the cache handled by the kernel at one time.
pub const MAX_READDIR_ENTRIES: usize = 50;

#[allow(dead_code)]
pub struct FileHandle {
    pub fh: u64,
    pub opened_at: Instant,
    pub last_offset: u64,
    pub flags: HandleFlags,
}

impl FileHandle {
    pub fn new(fh: u64, flags: HandleFlags) -> Self {
        Self {
            fh,
            opened_at: Instant::now(),
            last_offset: 0,
            flags,
        }
    }
}

#[allow(dead_code)]
#[derive(Clone, Copy)]
pub struct HandleFlags {
    read: bool,
    write: bool,
}

impl HandleFlags {
    pub const fn new(read: bool, write: bool) -> Self {
        Self { read, write }
    }
}

/// Directory handle for caching directory listing during opendir-releasedir lifecycle
#[derive(Clone)]
pub struct DirHandle {
    pub ino: i64,
    pub entries: Vec<DirEntry>,
    #[allow(dead_code)]
    pub opened_at: Instant,
}

impl DirHandle {
    pub fn new(ino: i64, entries: Vec<DirEntry>) -> Self {
        Self {
            ino,
            entries,
            opened_at: Instant::now(),
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
