//! Thin wrappers around [`MetaLayer`] methods used by the VFS.
//!
//! Every method here converts `MetaError` → `VfsError` via `VfsError::from`,
//! keeping the call sites in `fs.rs` free of repetitive boilerplate.
//! The three composite helpers (`meta_lookup_required`, `meta_stat_required`,
//! `meta_lookup_path_required`) that were previously at the bottom of `fs.rs` live
//! here as well, since they are purely metadata-layer concerns.

use crate::chunk::store::BlockStore;
use crate::meta::MetaLayer;
use crate::meta::file_lock::{FileLockInfo, FileLockQuery, FileLockRange, FileLockType};
use crate::meta::store::{
    AclRule, DirEntry, FileAttr, FileType, SetAttrFlags, SetAttrRequest, StatFsSnapshot,
};
use crate::vfs::error::{PathHint, VfsError};
use crate::vfs::fs::VFS;
use crate::vfs::handles::DirHandle;

fn meta_err_to_vfs(err: crate::meta::store::MetaError) -> VfsError {
    VfsError::from_meta(PathHint::none(), err)
}

impl<S, M> VFS<S, M>
where
    S: BlockStore + Send + Sync + 'static,
    M: MetaLayer + Send + Sync + 'static,
{
    // ------------------------------------------------------------------
    // Single-inode attribute queries
    // ------------------------------------------------------------------

    /// Fetch attributes for `ino`, returning `None` when the inode is absent.
    pub(super) async fn meta_stat(&self, ino: i64) -> Result<Option<FileAttr>, VfsError> {
        self.meta_layer().stat(ino).await.map_err(VfsError::from)
    }

    /// Fetch fresh (uncached) attributes for `ino`, returning `None` when absent.
    pub(super) async fn meta_stat_fresh(&self, ino: i64) -> Result<Option<FileAttr>, VfsError> {
        self.meta_layer()
            .stat_fresh(ino)
            .await
            .map_err(VfsError::from)
    }

    // ------------------------------------------------------------------
    // Directory / name lookups
    // ------------------------------------------------------------------

    /// Look up `name` under `parent`, returning `None` when absent.
    pub(super) async fn meta_lookup(
        &self,
        parent: i64,
        name: &str,
    ) -> Result<Option<i64>, VfsError> {
        self.meta_layer()
            .lookup(parent, name)
            .await
            .map_err(VfsError::from)
    }

    pub(super) async fn meta_lookup_required(
        &self,
        parent: i64,
        name: &str,
        hint: PathHint,
    ) -> Result<i64, VfsError> {
        self.meta_layer()
            .lookup(parent, name)
            .await
            .map_err(VfsError::from)?
            .ok_or_else(|| VfsError::NotFound { path: hint })
    }

    /// Resolve an absolute `path` to `(ino, FileType)`, returning `None` when absent.
    pub(super) async fn meta_lookup_path(
        &self,
        path: &str,
    ) -> Result<Option<(i64, FileType)>, VfsError> {
        self.meta_layer()
            .lookup_path(path)
            .await
            .map_err(VfsError::from)
    }

    // ------------------------------------------------------------------
    // Directory / name lookups (required variants)
    // ------------------------------------------------------------------

    /// Resolve `path` to `(ino, FileType)`, returning `VfsError::NotFound` when absent.
    pub(super) async fn meta_lookup_path_required(
        &self,
        path: &str,
    ) -> Result<(i64, FileType), VfsError> {
        self.meta_lookup_path(path)
            .await?
            .ok_or_else(|| VfsError::NotFound {
                path: PathHint::some(path),
            })
    }

    // ------------------------------------------------------------------
    // Directory mutation
    // ------------------------------------------------------------------

    pub(super) async fn meta_mkdir(&self, parent: i64, name: String) -> Result<i64, VfsError> {
        self.meta_layer()
            .mkdir(parent, name)
            .await
            .map_err(VfsError::from)
    }

    pub(super) async fn meta_rmdir(&self, parent: i64, name: &str) -> Result<(), VfsError> {
        self.meta_layer()
            .rmdir(parent, name)
            .await
            .map_err(VfsError::from)
    }

    pub(super) async fn meta_readdir(&self, ino: i64) -> Result<Vec<DirEntry>, VfsError> {
        self.meta_layer()
            .readdir(ino)
            .await
            .map_err(meta_err_to_vfs)
    }

    /// Open a directory handle for `ino`.
    pub(super) async fn meta_opendir(&self, ino: i64) -> Result<DirHandle, VfsError> {
        self.meta_layer()
            .opendir(ino)
            .await
            .map_err(|err| VfsError::from_meta(PathHint::none(), err))
    }

    // ------------------------------------------------------------------
    // File creation / linking
    // ------------------------------------------------------------------

    pub(super) async fn meta_create_file(
        &self,
        parent: i64,
        name: String,
    ) -> Result<i64, VfsError> {
        self.meta_layer()
            .create_file(parent, name)
            .await
            .map_err(VfsError::from)
    }

    pub(super) async fn meta_link(
        &self,
        ino: i64,
        parent: i64,
        name: &str,
    ) -> Result<FileAttr, VfsError> {
        self.meta_layer()
            .link(ino, parent, name)
            .await
            .map_err(VfsError::from)
    }

    pub(super) async fn meta_symlink(
        &self,
        parent: i64,
        name: &str,
        target: &str,
    ) -> Result<(i64, FileAttr), VfsError> {
        self.meta_layer()
            .symlink(parent, name, target)
            .await
            .map_err(VfsError::from)
    }

    pub(super) async fn meta_unlink(&self, parent: i64, name: &str) -> Result<(), VfsError> {
        self.meta_layer()
            .unlink(parent, name)
            .await
            .map_err(VfsError::from)
    }

    // ------------------------------------------------------------------
    // Rename operations
    // ------------------------------------------------------------------

    pub(super) async fn meta_rename(
        &self,
        old_parent: i64,
        old_name: &str,
        new_parent: i64,
        new_name: String,
    ) -> Result<(), VfsError> {
        self.meta_layer()
            .rename(old_parent, old_name, new_parent, new_name)
            .await
            .map_err(VfsError::from)
    }

    pub(super) async fn meta_rename_exchange(
        &self,
        old_parent: i64,
        old_name: &str,
        new_parent: i64,
        new_name: &str,
    ) -> Result<(), VfsError> {
        self.meta_layer()
            .rename_exchange(old_parent, old_name, new_parent, new_name)
            .await
            .map_err(VfsError::from)
    }

    // ------------------------------------------------------------------
    // Attribute mutation
    // ------------------------------------------------------------------

    pub(super) async fn meta_set_attr(
        &self,
        ino: i64,
        req: &SetAttrRequest,
        flags: SetAttrFlags,
    ) -> Result<FileAttr, VfsError> {
        self.meta_layer()
            .set_attr(ino, req, flags)
            .await
            .map_err(VfsError::from)
    }

    pub(super) async fn meta_chmod(&self, ino: i64, new_mode: u32) -> Result<FileAttr, VfsError> {
        self.meta_layer()
            .chmod(ino, new_mode)
            .await
            .map_err(VfsError::from)
    }

    pub(super) async fn meta_chown(
        &self,
        ino: i64,
        uid: Option<u32>,
        gid: Option<u32>,
    ) -> Result<FileAttr, VfsError> {
        self.meta_layer()
            .chown(ino, uid, gid)
            .await
            .map_err(VfsError::from)
    }

    pub(super) async fn meta_truncate(
        &self,
        ino: i64,
        size: u64,
        chunk_size: u64,
    ) -> Result<(), VfsError> {
        self.meta_layer()
            .truncate(ino, size, chunk_size)
            .await
            .map_err(VfsError::from)
    }

    // ------------------------------------------------------------------
    // Symlink content
    // ------------------------------------------------------------------

    pub(super) async fn meta_read_symlink(&self, ino: i64) -> Result<String, VfsError> {
        self.meta_layer()
            .read_symlink(ino)
            .await
            .map_err(VfsError::from)
    }

    pub(super) async fn meta_get_dir_parent(&self, ino: i64) -> Result<Option<i64>, VfsError> {
        self.meta_layer()
            .get_dir_parent(ino)
            .await
            .map_err(VfsError::from)
    }

    pub(super) async fn meta_get_paths(&self, ino: i64) -> Result<Vec<String>, VfsError> {
        self.meta_layer()
            .get_paths(ino)
            .await
            .map_err(VfsError::from)
    }

    // ------------------------------------------------------------------
    // File locks
    // ------------------------------------------------------------------

    pub(super) async fn meta_get_plock(
        &self,
        inode: i64,
        query: &FileLockQuery,
    ) -> Result<FileLockInfo, VfsError> {
        self.meta_layer()
            .get_plock(inode, query)
            .await
            .map_err(|err| VfsError::from_meta(PathHint::none(), err))
    }

    pub(super) async fn meta_set_plock(
        &self,
        inode: i64,
        owner: i64,
        block: bool,
        lock_type: FileLockType,
        range: FileLockRange,
        pid: u32,
    ) -> Result<(), VfsError> {
        self.meta_layer()
            .set_plock(inode, owner, block, lock_type, range, pid)
            .await
            .map_err(|err| VfsError::from_meta(PathHint::none(), err))
    }

    // ------------------------------------------------------------------
    // Extended attributes
    // ------------------------------------------------------------------

    pub(super) async fn meta_set_xattr(
        &self,
        inode: i64,
        name: &str,
        value: &[u8],
        flags: u32,
    ) -> Result<(), VfsError> {
        self.meta_layer()
            .set_xattr(inode, name, value, flags)
            .await
            .map_err(|err| VfsError::from_meta(PathHint::none(), err))
    }

    pub(super) async fn meta_get_xattr(
        &self,
        inode: i64,
        name: &str,
    ) -> Result<Option<Vec<u8>>, VfsError> {
        self.meta_layer()
            .get_xattr(inode, name)
            .await
            .map_err(|err| VfsError::from_meta(PathHint::none(), err))
    }

    pub(super) async fn meta_list_xattr(&self, inode: i64) -> Result<Vec<String>, VfsError> {
        self.meta_layer()
            .list_xattr(inode)
            .await
            .map_err(|err| VfsError::from_meta(PathHint::none(), err))
    }

    pub(super) async fn meta_remove_xattr(&self, inode: i64, name: &str) -> Result<(), VfsError> {
        self.meta_layer()
            .remove_xattr(inode, name)
            .await
            .map_err(|err| VfsError::from_meta(PathHint::none(), err))
    }

    // ------------------------------------------------------------------
    // ACL
    // ------------------------------------------------------------------

    pub(super) async fn meta_set_acl(&self, inode: i64, rule: AclRule) -> Result<(), VfsError> {
        self.meta_layer()
            .set_acl(inode, rule)
            .await
            .map_err(|err| VfsError::from_meta(PathHint::none(), err))
    }

    pub(super) async fn meta_get_acl(
        &self,
        inode: i64,
        acl_type: u8,
        acl_id: u32,
    ) -> Result<Option<AclRule>, VfsError> {
        self.meta_layer()
            .get_acl(inode, acl_type, acl_id)
            .await
            .map_err(|err| VfsError::from_meta(PathHint::none(), err))
    }

    // ------------------------------------------------------------------
    // Filesystem statistics
    // ------------------------------------------------------------------

    pub(super) async fn meta_stat_fs(&self) -> Result<StatFsSnapshot, VfsError> {
        self.meta_layer()
            .stat_fs()
            .await
            .map_err(|err| VfsError::from_meta(PathHint::none(), err))
    }

    // ------------------------------------------------------------------
    // Composite helpers (moved from fs.rs)
    // ------------------------------------------------------------------

    /// Stat `ino`, returning `VfsError::NotFound` when the inode is absent.
    pub(super) async fn meta_stat_required(
        &self,
        ino: i64,
        hint: PathHint,
    ) -> Result<FileAttr, VfsError> {
        self.meta_stat(ino)
            .await?
            .ok_or_else(|| VfsError::NotFound { path: hint })
    }
}
