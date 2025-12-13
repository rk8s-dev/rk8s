use async_trait::async_trait;

use crate::chuck::SliceDesc;
use crate::meta::client::session::SessionInfo;
use crate::meta::store::{
    DirEntry, FileAttr, FileType, MetaError, OpenFlags, SetAttrFlags, SetAttrRequest,
    StatFsSnapshot,
};

/// High-level metadata facade used by the VFS and daemon layers.
///
/// This trait intentionally mirrors the shape of JuiceFS' `Meta` interface.
/// The goal is to expose path-friendly helpers (with caching, session
/// management, etc.) while chunk IO or maintenance workers continue to talk to
/// the raw [`MetaStore`] directly. Implementations may return
/// `MetaError::NotImplemented` for operations that have not landed yet, but the
/// signatures are provided up front to ease future parity work.
#[allow(dead_code)]
#[async_trait]
pub trait MetaLayer: Send + Sync {
    /// Optional human readable backend name.
    fn name(&self) -> &'static str {
        "meta-layer"
    }

    /// Returns / mutates the logical root inode alias used by chroot.
    fn root_ino(&self) -> i64;
    fn chroot(&self, inode: i64);

    /// Performs backend initialization / schema checks.
    async fn initialize(&self) -> Result<(), MetaError>;
    async fn stat_fs(&self) -> Result<StatFsSnapshot, MetaError>;

    // ---------- Core path operations ----------
    async fn stat(&self, ino: i64) -> Result<Option<FileAttr>, MetaError>;
    async fn lookup(&self, parent: i64, name: &str) -> Result<Option<i64>, MetaError>;
    async fn lookup_path(&self, path: &str) -> Result<Option<(i64, FileType)>, MetaError>;
    async fn readdir(&self, ino: i64) -> Result<Vec<DirEntry>, MetaError>;
    async fn mkdir(&self, parent: i64, name: String) -> Result<i64, MetaError>;
    async fn rmdir(&self, parent: i64, name: &str) -> Result<(), MetaError>;
    async fn create_file(&self, parent: i64, name: String) -> Result<i64, MetaError>;
    async fn link(&self, ino: i64, parent: i64, name: &str) -> Result<FileAttr, MetaError>;
    async fn symlink(
        &self,
        parent: i64,
        name: &str,
        target: &str,
    ) -> Result<(i64, FileAttr), MetaError>;
    async fn unlink(&self, parent: i64, name: &str) -> Result<(), MetaError>;
    async fn rename(
        &self,
        old_parent: i64,
        old_name: &str,
        new_parent: i64,
        new_name: String,
    ) -> Result<(), MetaError>;
    async fn set_file_size(&self, ino: i64, size: u64) -> Result<(), MetaError>;
    async fn get_parent(&self, ino: i64) -> Result<Option<i64>, MetaError>;
    async fn get_name(&self, ino: i64) -> Result<Option<String>, MetaError>;
    async fn get_path(&self, ino: i64) -> Result<Option<String>, MetaError>;
    async fn read_symlink(&self, ino: i64) -> Result<String, MetaError>;

    // ---------- Attribute + handle helpers ----------
    async fn set_attr(
        &self,
        ino: i64,
        req: &SetAttrRequest,
        flags: SetAttrFlags,
    ) -> Result<FileAttr, MetaError>;
    async fn open(&self, ino: i64, flags: OpenFlags) -> Result<FileAttr, MetaError>;
    async fn close(&self, ino: i64) -> Result<(), MetaError>;

    // ---------- Chunk + ID utilities ----------
    async fn get_deleted_files(&self) -> Result<Vec<i64>, MetaError>;
    async fn remove_file_metadata(&self, ino: i64) -> Result<(), MetaError>;
    async fn get_slices(&self, chunk_id: u64) -> Result<Vec<SliceDesc>, MetaError>;
    async fn append_slice(&self, chunk_id: u64, slice: SliceDesc) -> Result<(), MetaError>;
    async fn next_id(&self, key: &str) -> Result<i64, MetaError>;

    // ---------- Session lifecycle ----------
    async fn start_session(&self, session_info: SessionInfo) -> Result<(), MetaError>;
    async fn shutdown_session(&self) -> Result<(), MetaError>;
}
