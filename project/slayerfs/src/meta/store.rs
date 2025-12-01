//! Metadata store abstract interface
//!
//! Defines unified interface for filesystem metadata operations
use crate::chuck::SliceDesc;
use crate::meta::entities::content_meta::EntryType;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::SystemTime;

/// File type enumeration
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileType {
    File,
    Dir,
    Symlink,
}

impl From<EntryType> for FileType {
    fn from(entry_type: EntryType) -> Self {
        match entry_type {
            EntryType::File => FileType::File,
            EntryType::Directory => FileType::Dir,
            EntryType::Symlink => FileType::Symlink,
        }
    }
}

/// File attributes
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct FileAttr {
    pub ino: i64,
    pub size: u64,
    pub kind: FileType,
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    pub atime: i64,
    pub mtime: i64,
    pub ctime: i64,
    pub nlink: u32,
}

/// Bitmask describing which fields should be updated in a `set_attr` call.
#[derive(Debug, Clone, Copy, Default)]
#[allow(dead_code)]
pub struct SetAttrRequest {
    pub mode: Option<u32>,
    pub uid: Option<u32>,
    pub gid: Option<u32>,
    pub size: Option<u64>,
    pub atime: Option<i64>,
    pub mtime: Option<i64>,
    pub ctime: Option<i64>,
    pub flags: Option<u32>,
}

bitflags::bitflags! {
    /// Additional flags that control set-attribute semantics.
    #[allow(dead_code)]
    pub struct SetAttrFlags: u32 {
        const CLEAR_SUID = 0b0001;
        const CLEAR_SGID = 0b0010;
        const SET_ATIME_NOW = 0b0100;
        const SET_MTIME_NOW = 0b1000;
    }
}

bitflags::bitflags! {
    /// POSIX-style open flags translated for the metadata store.
    #[allow(dead_code)]
    pub struct OpenFlags: u32 {
        const RDONLY = 0b0001;
        const WRONLY = 0b0010;
        const RDWR   = 0b0011;
        const APPEND = 0b0100;
        const TRUNC  = 0b1000;
        const CREATE = 0b0001_0000;
    }
}

/// Describes a single chunk slice returned by the store.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ChunkSlice {
    pub id: u64,
    pub offset: u64,
    pub length: u32,
    pub chunk_index: u32,
}

/// Payload used when writing a slice to the store.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ChunkWrite {
    pub slice: ChunkSlice,
    pub data_len: u64,
    pub mtime: i64,
}

/// Result of a write operation including accounting deltas.
#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct WriteOutcome {
    pub updated_attr: Option<FileAttr>,
    pub space_delta: i64,
    pub inode_delta: i64,
}

/// Snapshot returned by `stat_fs` providing capacity/inode information.
#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct StatFsSnapshot {
    pub total_space: u64,
    pub available_space: u64,
    pub used_inodes: u64,
    pub available_inodes: u64,
}

/// Directory entry
#[derive(Debug, Clone)]
pub struct DirEntry {
    pub name: String,
    pub ino: i64,
    pub kind: FileType,
}

/// Extended directory entry used by readdir+ style operations
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct DirEntryPlus {
    pub entry: DirEntry,
    pub attr: Option<FileAttr>,
}

/// Directory statistics used for quota/accounting updates
#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct DirStat {
    pub space: i64,
    pub inodes: i64,
}

/// Quota information for a key (user/group/project)
#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct Quota {
    pub limit_space: Option<i64>,
    pub limit_inodes: Option<i64>,
    pub used_space: i64,
    pub used_inodes: i64,
}

/// Incremental quota delta awaiting flush
#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct QuotaDelta {
    pub key: u64,
    pub space_delta: i64,
    pub inode_delta: i64,
}

/// Metadata engine runtime statistics snapshot
#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct VolumeStat {
    pub space_used: i64,
    pub inode_count: i64,
}

/// Session descriptor returned by session queries.
///
/// A `SessionInfo` represents a lightweight record describing a single
/// metadata client's session registration in the metadata backend. Sessions
/// are used by the metadata service to track active clients, enable safe
/// garbage-collection of transient state (for example, delayed deletes),
/// and to coordinate cross-process maintenance tasks.
///
/// Fields:
/// - `id`: opaque session identifier issued by the backend. Implementations
///   should ensure uniqueness within the metadata service lifetime and may
///   reuse identifiers after long-term cleanup.
/// - `info`: optional payload provided by the client at registration time.
///   Typical content is serialized client metadata (for example, mount
///   options, pid, hostname) but the bytes are intentionally uninterpreted
///   by the store interface.
/// - `updated_at`: wall-clock timestamp (system time) when the session
///   record was last refreshed. The session manager relies on this value to
///   detect stale sessions; therefore implementations should update it when
///   a heartbeat/refresh operation succeeds.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[allow(dead_code)]
pub struct SessionInfo {
    pub id: u64,
    pub info: Vec<u8>,
    pub updated_at: SystemTime,
}

/// ACL rule placeholder (to be fleshed out once ACL storage lands)
#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct AclRule {
    pub acl_type: u8,
    pub qualifier: u32,
    pub permissions: u32,
}

/// Options used by metadata dump API
#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct DumpOption {
    pub include_deleted: bool,
    pub limit: Option<usize>,
}

/// Result row produced by metadata dump streaming API
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct DumpRecord {
    pub inode: i64,
    pub path: Option<String>,
    pub attr: FileAttr,
}

/// Options used during metadata bulk load operations
#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct LoadOption {
    pub allow_conflicts: bool,
}

/// Visitor trait for streaming APIs (dump/scan).
///
/// Several metadata operations (for example `dump`) provide streaming-style
/// results. The `Visitor<T>` trait defines a simple callback contract used by
/// these APIs: the store implementation will repeatedly call `visit` with
/// items produced by the operation. Implementers should keep the following
/// semantics in mind:
///
/// - `visit` is called synchronously from the context of the store call; if
///   the visitor needs to perform heavy work consider buffering or yielding
///   to avoid blocking the store's internal task.
/// - Returning `Err(MetaError)` from `visit` signals the store to abort the
///   streaming operation and propagate the error to the caller.
/// - The visitor may be stateful; it receives `&mut self` and can therefore
///   accumulate results across multiple `visit` calls.
#[allow(dead_code)]
pub trait Visitor<T>: Send {
    fn visit(&mut self, item: T) -> Result<(), MetaError>;
}

/// Metadata operation errors
#[derive(Debug, thiserror::Error)]
#[allow(dead_code)]
pub enum MetaError {
    #[error("Entry not found: {0}")]
    NotFound(i64),

    #[error("Parent directory not found: {0}")]
    ParentNotFound(i64),

    #[error("Entry already exists: {name} in parent {parent}")]
    AlreadyExists { parent: i64, name: String },

    #[error("Not a directory: {0}")]
    NotDirectory(i64),

    #[error("Directory not empty: {0}")]
    DirectoryNotEmpty(i64),

    #[error("Invalid path: {0}")]
    InvalidPath(String),

    #[error("Operation not supported: {0}")]
    NotSupported(String),

    #[error("Not implemented")]
    NotImplemented,

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("continue retry")]
    ContinueRetry,

    #[error("error: max retries exceeded")]
    MaxRetriesExceeded,

    #[error("Database error: {0}")]
    Database(#[from] sea_orm::DbErr),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Config error: {0}")]
    Config(String),

    #[error("Session not found")]
    SessionNotFound,

    #[error("Invalid handle: {0}")]
    InvalidHandle(u64),

    #[error("error: {0}")]
    Anyhow(#[from] anyhow::Error),
}

/// Metadata store abstract interface.
///
/// `MetaStore` defines the contract required from a metadata backend that
/// stores filesystem namespace information and file layout metadata. The
/// trait is intentionally broad so different concrete backends (for
/// example an embedded SQLite store, a distributed KV-backed store, or a
/// mocked in-memory store) can be used interchangeably by higher-level
/// components.
///
/// Implementers should honor the following conventions:
/// - Methods returning `Result<T, MetaError>` should map backend-specific
///   failures into the `MetaError` variants defined in this module. Network
///   or IO errors must be wrapped in `MetaError::Io` or `MetaError::Database`
///   as appropriate.
/// - Methods that have default `NotImplemented` implementations are
///   optional; callers must tolerate `MetaError::NotImplemented` where
///   documented. Core filesystem operations (lookup/stat/read/write-related)
///   should be implemented by production backends.
/// - Date/time semantics: timestamps passed or returned (for example
///   `SystemTime` fields) are wall-clock times; backends running on
///   different hosts should ensure reasonable clock synchronization if they
///   participate in shared state.
///
#[async_trait]
#[auto_impl::auto_impl(&, std::sync::Arc)]
/// The trait uses `async_trait` to allow async method implementations and
/// `auto_impl` to conveniently implement the trait for shared references and
/// `Arc<T>` wrappers.
#[allow(dead_code)]
#[allow(clippy::too_many_arguments)]
pub trait MetaStore: Send + Sync {
    /// Human readable backend name (for diagnostics and logging)
    fn name(&self) -> &'static str {
        "meta-store"
    }

    async fn stat(&self, ino: i64) -> Result<Option<FileAttr>, MetaError>;

    async fn lookup(&self, parent: i64, name: &str) -> Result<Option<i64>, MetaError>;

    async fn lookup_path(&self, path: &str) -> Result<Option<(i64, FileType)>, MetaError>;

    async fn readdir(&self, ino: i64) -> Result<Vec<DirEntry>, MetaError>;

    async fn mkdir(&self, parent: i64, name: String) -> Result<i64, MetaError>;

    async fn rmdir(&self, parent: i64, name: &str) -> Result<(), MetaError>;

    async fn create_file(&self, parent: i64, name: String) -> Result<i64, MetaError>;

    async fn unlink(&self, parent: i64, name: &str) -> Result<(), MetaError>;

    async fn rename(
        &self,
        old_parent: i64,
        old_name: &str,
        new_parent: i64,
        new_name: String,
    ) -> Result<(), MetaError>;

    async fn set_file_size(&self, ino: i64, size: u64) -> Result<(), MetaError>;

    /// get the node's parent inode
    async fn get_parent(&self, ino: i64) -> Result<Option<i64>, MetaError>;

    /// get the node's name in its parent directory
    async fn get_name(&self, ino: i64) -> Result<Option<String>, MetaError>;

    /// get the inode's full path (from the root directory)
    async fn get_path(&self, ino: i64) -> Result<Option<String>, MetaError>;

    fn root_ino(&self) -> i64;

    async fn initialize(&self) -> Result<(), MetaError>;

    /// Returns all file inodes marked for deletion (for garbage collection)
    async fn get_deleted_files(&self) -> Result<Vec<i64>, MetaError>;

    async fn remove_file_metadata(&self, ino: i64) -> Result<(), MetaError>;

    async fn get_slices(&self, chunk_id: u64) -> Result<Vec<SliceDesc>, MetaError>;

    async fn append_slice(&self, chunk_id: u64, slice: SliceDesc) -> Result<(), MetaError>;

    async fn next_id(&self, key: &str) -> Result<i64, MetaError>;
    /// Allow downcasting to concrete types
    fn as_any(&self) -> &dyn std::any::Any;

    // ---------- Counter & statistics helpers ----------

    async fn get_counter(&self, name: &str) -> Result<i64, MetaError> {
        let _ = name;
        Err(MetaError::NotImplemented)
    }

    async fn incr_counter(&self, name: &str, delta: i64) -> Result<i64, MetaError> {
        let _ = (name, delta);
        Err(MetaError::NotImplemented)
    }

    async fn set_counter_if_small(
        &self,
        name: &str,
        value: i64,
        diff: i64,
    ) -> Result<bool, MetaError> {
        let _ = (name, value, diff);
        Err(MetaError::NotImplemented)
    }

    async fn update_volume_stat(&self, delta: DirStat) -> Result<(), MetaError> {
        let _ = delta;
        Err(MetaError::NotImplemented)
    }

    async fn flush_volume_stat(&self) -> Result<VolumeStat, MetaError> {
        Err(MetaError::NotImplemented)
    }

    // ---------- Session lifecycle ----------

    async fn new_session(&self, payload: &[u8], update: bool) -> Result<(), MetaError> {
        let _ = (payload, update);
        Err(MetaError::NotImplemented)
    }

    async fn refresh_session(&self) -> Result<(), MetaError> {
        Err(MetaError::NotImplemented)
    }

    /// Refresh session by client identifier (hostname + process_id)
    /// This provides more precise session management than the generic refresh_session
    async fn refresh_session_by_id(
        &self,
        session_id: &crate::meta::client::session::SessionId,
    ) -> Result<(), MetaError> {
        let _ = session_id;
        // Default implementation falls back to the original method
        self.refresh_session().await
    }

    async fn find_stale_sessions(
        &self,
        limit: Option<usize>,
    ) -> Result<Vec<SessionInfo>, MetaError> {
        let _ = limit;
        Err(MetaError::NotImplemented)
    }

    async fn clean_stale_session(&self, session_id: u64) -> Result<(), MetaError> {
        let _ = session_id;
        Err(MetaError::NotImplemented)
    }

    /// Clean up session by SessionId (hostname + process_id)
    async fn clean_session_by_id(
        &self,
        session_id: &crate::meta::client::session::SessionId,
    ) -> Result<(), MetaError> {
        let _ = session_id;
        Err(MetaError::NotImplemented)
    }

    // ---------- Attribute / handle management (proposed extensions) ----------

    async fn set_attr(
        &self,
        ino: i64,
        req: &SetAttrRequest,
        flags: SetAttrFlags,
    ) -> Result<FileAttr, MetaError> {
        let _ = (ino, req, flags);
        Err(MetaError::NotImplemented)
    }

    async fn open(&self, ino: i64, flags: OpenFlags) -> Result<FileAttr, MetaError> {
        let _ = (ino, flags);
        Err(MetaError::NotImplemented)
    }

    async fn close(&self, ino: i64) -> Result<(), MetaError> {
        let _ = ino;
        Err(MetaError::NotImplemented)
    }

    async fn link(&self, ino: i64, parent: i64, name: &str) -> Result<FileAttr, MetaError> {
        let _ = (ino, parent, name);
        Err(MetaError::NotImplemented)
    }

    async fn symlink(
        &self,
        parent: i64,
        name: &str,
        target: &str,
    ) -> Result<(i64, FileAttr), MetaError> {
        let _ = (parent, name, target);
        Err(MetaError::NotImplemented)
    }

    async fn read_symlink(&self, ino: i64) -> Result<String, MetaError> {
        let _ = ino;
        Err(MetaError::NotImplemented)
    }

    async fn stat_fs(&self) -> Result<StatFsSnapshot, MetaError> {
        Err(MetaError::NotImplemented)
    }
    // ---------- Garbage collection helpers ----------

    async fn delete_sustained_inode(&self, session_id: u64, inode: i64) -> Result<(), MetaError> {
        let _ = (session_id, inode);
        Err(MetaError::NotImplemented)
    }

    async fn delete_file_data(&self, inode: i64, length: u64) -> Result<(), MetaError> {
        let _ = (inode, length);
        Err(MetaError::NotImplemented)
    }

    async fn cleanup_slices(&self) -> Result<(), MetaError> {
        Err(MetaError::NotImplemented)
    }

    async fn cleanup_delayed_slices(&self, edge_ts: i64) -> Result<i32, MetaError> {
        let _ = edge_ts;
        Err(MetaError::NotImplemented)
    }

    async fn delete_slice(&self, slice_id: u64, size: u32) -> Result<(), MetaError> {
        let _ = (slice_id, size);
        Err(MetaError::NotImplemented)
    }

    // ---------- Directory maintenance ----------

    async fn clone_entry(
        &self,
        src: i64,
        parent: i64,
        name: &str,
        ino: i64,
        attr: &mut FileAttr,
        cmode: u8,
        cumask: u16,
        top: bool,
    ) -> Result<(), MetaError> {
        let _ = (src, parent, name, ino, attr, cmode, cumask, top);
        Err(MetaError::NotImplemented)
    }

    async fn attach_dir_node(&self, parent: i64, dst: i64, name: &str) -> Result<(), MetaError> {
        let _ = (parent, dst, name);
        Err(MetaError::NotImplemented)
    }

    async fn find_detached_nodes(&self, since: SystemTime) -> Result<Vec<i64>, MetaError> {
        let _ = since;
        Err(MetaError::NotImplemented)
    }

    async fn cleanup_detached_node(&self, inode: i64) -> Result<(), MetaError> {
        let _ = inode;
        Err(MetaError::NotImplemented)
    }

    /// Returns directory statistics map keyed by parent inode.
    async fn get_parents(&self, inode: i64) -> Result<HashMap<i64, i32>, MetaError> {
        let _ = inode;
        Err(MetaError::NotImplemented)
    }

    async fn update_dir_stat(&self, batch: HashMap<i64, DirStat>) -> Result<(), MetaError> {
        let _ = batch;
        Err(MetaError::NotImplemented)
    }

    async fn get_dir_stat(&self, inode: i64, try_sync: bool) -> Result<Option<DirStat>, MetaError> {
        let _ = (inode, try_sync);
        Err(MetaError::NotImplemented)
    }

    async fn sync_dir_stat(&self, inode: i64) -> Result<Option<DirStat>, MetaError> {
        let _ = inode;
        Err(MetaError::NotImplemented)
    }

    async fn sync_volume_stat(&self) -> Result<VolumeStat, MetaError> {
        Err(MetaError::NotImplemented)
    }

    // ---------- Quota management ----------

    async fn get_quota(&self, qtype: u32, key: u64) -> Result<Option<Quota>, MetaError> {
        let _ = (qtype, key);
        Err(MetaError::NotImplemented)
    }

    async fn set_quota(&self, qtype: u32, key: u64, quota: Quota) -> Result<bool, MetaError> {
        let _ = (qtype, key, quota);
        Err(MetaError::NotImplemented)
    }

    async fn delete_quota(&self, qtype: u32, key: u64) -> Result<(), MetaError> {
        let _ = (qtype, key);
        Err(MetaError::NotImplemented)
    }

    async fn load_quotas(
        &self,
    ) -> Result<
        (
            HashMap<u64, Quota>,
            HashMap<u64, Quota>,
            HashMap<u64, Quota>,
        ),
        MetaError,
    > {
        Err(MetaError::NotImplemented)
    }

    async fn flush_quotas(&self, deltas: &[QuotaDelta]) -> Result<(), MetaError> {
        let _ = deltas;
        Err(MetaError::NotImplemented)
    }

    // ---------- Enhanced readdir / dump / load ----------

    async fn readdir_plus(
        &self,
        ino: i64,
        limit: Option<usize>,
    ) -> Result<Vec<DirEntryPlus>, MetaError> {
        let _ = (ino, limit);
        Err(MetaError::NotImplemented)
    }

    async fn dump(
        &self,
        opt: DumpOption,
        visitor: &mut dyn Visitor<DumpRecord>,
    ) -> Result<(), MetaError> {
        let _ = (opt, visitor);
        Err(MetaError::NotImplemented)
    }

    async fn load(&self, opt: LoadOption, data: &[u8]) -> Result<(), MetaError> {
        let _ = (opt, data);
        Err(MetaError::NotImplemented)
    }

    // ---------- Extended attribute & ACL ----------

    async fn set_xattr(
        &self,
        inode: i64,
        name: &str,
        value: &[u8],
        flags: u32,
    ) -> Result<(), MetaError> {
        let _ = (inode, name, value, flags);
        Err(MetaError::NotImplemented)
    }

    async fn remove_xattr(&self, inode: i64, name: &str) -> Result<(), MetaError> {
        let _ = (inode, name);
        Err(MetaError::NotImplemented)
    }

    async fn cache_acls(&self) -> Result<(), MetaError> {
        Err(MetaError::NotImplemented)
    }

    async fn set_acl(&self, inode: i64, rule: AclRule) -> Result<(), MetaError> {
        let _ = (inode, rule);
        Err(MetaError::NotImplemented)
    }

    async fn get_acl(
        &self,
        inode: i64,
        acl_type: u8,
        acl_id: u32,
    ) -> Result<Option<AclRule>, MetaError> {
        let _ = (inode, acl_type, acl_id);
        Err(MetaError::NotImplemented)
    }

    // ---------- File data path helpers ----------

    async fn read_slices(&self, inode: i64, chunk_index: u32) -> Result<Vec<SliceDesc>, MetaError> {
        let _ = (inode, chunk_index);
        Err(MetaError::NotImplemented)
    }

    async fn write_slice(
        &self,
        inode: i64,
        chunk_index: u32,
        offset: u32,
        slice: SliceDesc,
        mtime: SystemTime,
        num_slices: &mut i32,
        delta: &mut DirStat,
        attr: &mut FileAttr,
    ) -> Result<(), MetaError> {
        let _ = (
            inode,
            chunk_index,
            offset,
            slice,
            mtime,
            num_slices,
            delta,
            attr,
        );
        Err(MetaError::NotImplemented)
    }

    async fn truncate_file(
        &self,
        inode: i64,
        flags: u8,
        length: u64,
        delta: &mut DirStat,
        attr: &mut FileAttr,
        skip_perm_check: bool,
    ) -> Result<(), MetaError> {
        let _ = (inode, flags, length, delta, attr, skip_perm_check);
        Err(MetaError::NotImplemented)
    }

    async fn fallocate_file(
        &self,
        inode: i64,
        mode: u8,
        offset: u64,
        size: u64,
        delta: &mut DirStat,
        attr: &mut FileAttr,
    ) -> Result<(), MetaError> {
        let _ = (inode, mode, offset, size, delta, attr);
        Err(MetaError::NotImplemented)
    }

    async fn compact_chunk(
        &self,
        inode: i64,
        index: u32,
        origin: &[u8],
        slices: &[SliceDesc],
        skipped: i32,
        pos: u32,
        id: u64,
        size: u32,
        delayed: &[u8],
    ) -> Result<(), MetaError> {
        let _ = (
            inode, index, origin, slices, skipped, pos, id, size, delayed,
        );
        Err(MetaError::NotImplemented)
    }
}
