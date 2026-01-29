use async_trait::async_trait;

use crate::chuck::SliceDesc;
use crate::meta::client::session::SessionInfo;
use crate::meta::file_lock::{FileLockInfo, FileLockQuery, FileLockRange, FileLockType};
use crate::meta::store::{
    DirEntry, FileAttr, FileType, MetaError, OpenFlags, SetAttrFlags, SetAttrRequest,
    StatFsSnapshot,
};
use crate::vfs::handles::DirHandle;

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

    /// Do `stat` but bypass the inode cache.
    async fn stat_fresh(&self, ino: i64) -> Result<Option<FileAttr>, MetaError>;

    async fn lookup(&self, parent: i64, name: &str) -> Result<Option<i64>, MetaError>;

    async fn lookup_path(&self, path: &str) -> Result<Option<(i64, FileType)>, MetaError>;

    async fn readdir(&self, ino: i64) -> Result<Vec<DirEntry>, MetaError>;

    async fn opendir(&self, ino: i64) -> Result<DirHandle, MetaError>;

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

    /// Atomically exchange two files (RENAME_EXCHANGE).
    /// Both entries must exist.
    async fn rename_exchange(
        &self,
        old_parent: i64,
        old_name: &str,
        new_parent: i64,
        new_name: &str,
    ) -> Result<(), MetaError>;

    /// Check if a rename operation would be allowed without performing it.
    async fn can_rename(
        &self,
        old_parent: i64,
        old_name: &str,
        new_parent: i64,
        new_name: &str,
    ) -> Result<(), MetaError> {
        // Default implementation - just try the basic validation
        let src_ino = self
            .lookup(old_parent, old_name)
            .await?
            .ok_or(MetaError::NotFound(old_parent))?;

        // Check if destination exists and validate replacement rules
        if let Some(dest_ino) = self.lookup(new_parent, new_name).await? {
            let src_attr = self
                .stat(src_ino)
                .await?
                .ok_or(MetaError::NotFound(src_ino))?;
            let dest_attr = self
                .stat(dest_ino)
                .await?
                .ok_or(MetaError::NotFound(dest_ino))?;

            match (src_attr.kind, dest_attr.kind) {
                // Directory replacing directory - check if empty
                (FileType::Dir, FileType::Dir) => {
                    let children = self.readdir(dest_ino).await?;
                    if !children.is_empty() {
                        return Err(MetaError::DirectoryNotEmpty(dest_ino));
                    }
                }
                // Directory replacing file/symlink - not allowed
                (FileType::Dir, FileType::File) | (FileType::Dir, FileType::Symlink) => {
                    return Err(MetaError::NotDirectory(dest_ino));
                }
                // File/symlink replacing directory - not allowed
                (FileType::File, FileType::Dir) | (FileType::Symlink, FileType::Dir) => {
                    return Err(MetaError::NotDirectory(dest_ino));
                }
                // File/symlink replacing file/symlink - allowed
                _ => {}
            }
        }

        Ok(())
    }

    /// Rename with extended flags support (similar to Linux renameat2).
    async fn rename_with_flags(
        &self,
        old_parent: i64,
        old_name: &str,
        new_parent: i64,
        new_name: String,
        flags: crate::vfs::fs::RenameFlags,
    ) -> Result<(), MetaError> {
        if flags.exchange {
            // Use atomic exchange implementation
            self.rename_exchange(old_parent, old_name, new_parent, &new_name)
                .await
        } else if flags.noreplace {
            // Check if destination exists
            if self.lookup(new_parent, &new_name).await?.is_some() {
                return Err(MetaError::AlreadyExists {
                    parent: new_parent,
                    name: new_name,
                });
            }
            self.rename(old_parent, old_name, new_parent, new_name)
                .await
        } else {
            // Default behavior
            self.rename(old_parent, old_name, new_parent, new_name)
                .await
        }
    }

    async fn set_file_size(&self, ino: i64, size: u64) -> Result<(), MetaError>;

    async fn extend_file_size(&self, ino: i64, size: u64) -> Result<(), MetaError>;

    async fn truncate(&self, ino: i64, size: u64, chunk_size: u64) -> Result<(), MetaError>;

    async fn get_names(&self, ino: i64) -> Result<Vec<(Option<i64>, String)>, MetaError>;

    async fn get_dentries(&self, ino: i64) -> Result<Vec<(i64, String)>, MetaError>;

    async fn get_dir_parent(&self, dir_ino: i64) -> Result<Option<i64>, MetaError>;

    async fn get_paths(&self, ino: i64) -> Result<Vec<String>, MetaError>;

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

    async fn write(
        &self,
        ino: i64,
        chunk_id: u64,
        slice: SliceDesc,
        new_size: u64,
    ) -> Result<(), MetaError>;

    // ---------- Metadata + ID utilities ----------
    async fn get_deleted_files(&self) -> Result<Vec<i64>, MetaError>;

    async fn remove_file_metadata(&self, ino: i64) -> Result<(), MetaError>;

    async fn get_slices(&self, chunk_id: u64) -> Result<Vec<SliceDesc>, MetaError>;

    async fn append_slice(&self, chunk_id: u64, slice: SliceDesc) -> Result<(), MetaError>;

    async fn next_id(&self, key: &str) -> Result<i64, MetaError>;

    // ---------- Session lifecycle ----------
    async fn start_session(&self, session_info: SessionInfo) -> Result<(), MetaError>;

    async fn shutdown_session(&self) -> Result<(), MetaError>;

    // ---------- File lock operations ----------
    async fn get_plock(&self, inode: i64, query: &FileLockQuery)
    -> Result<FileLockInfo, MetaError>;

    async fn set_plock(
        &self,
        inode: i64,
        owner: i64,
        block: bool,
        lock_type: FileLockType,
        range: FileLockRange,
        pid: u32,
    ) -> Result<(), MetaError>;
}
