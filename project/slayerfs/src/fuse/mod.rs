//! FUSE adapter and request handling
//! This module provides the FUSE (Filesystem in Userspace) integration for SlayerFS.
//! It implements the adapter and request handling logic required to expose the virtual filesystem
//! to the operating system via the FUSE protocol.
//!
//! Main components:
//! - `adapter`: Contains the FUSE adapter implementation.
//! - `mount`: Handles mounting the virtual filesystem using FUSE.
//! - Implementation of the `Filesystem` trait for `VFS`, enabling translation of FUSE requests
//!   into virtual filesystem operations.
//! - Helpers for attribute and file type conversion between VFS and FUSE representations.
//!
//! The module also includes platform-specific tests for mounting and basic operations,
//! and provides utilities for mapping VFS metadata to FUSE attributes.
pub(crate) mod adapter;
pub mod mount;
use crate::chuck::store::BlockStore;
use crate::meta::MetaLayer;
use crate::meta::file_lock::{FileLockQuery, FileLockRange, FileLockType};
use crate::meta::store::{MetaError, SetAttrFlags, SetAttrRequest};
use crate::posix::NAME_MAX;
use crate::vfs::error::VfsError;
use crate::vfs::fs::{FileAttr as VfsFileAttr, FileType as VfsFileType, VFS};
use bytes::Bytes;
use rfuse3::Errno;
use rfuse3::Result as FuseResult;
use rfuse3::raw::Request;
use rfuse3::raw::reply::{
    DirectoryEntry, DirectoryEntryPlus, ReplyAttr, ReplyCreated, ReplyData, ReplyDirectory,
    ReplyDirectoryPlus, ReplyEntry, ReplyInit, ReplyLock, ReplyOpen, ReplyStatFs, ReplyWrite,
    ReplyXAttr,
};
use std::ffi::{OsStr, OsString};
use std::time::Duration;

use futures_util::stream::{self, BoxStream};
use rfuse3::raw::Filesystem;
use rfuse3::{FileType as FuseFileType, SetAttr, Timestamp};
use tracing::{debug, error};
#[cfg(all(test, target_os = "linux"))]
mod mount_tests {
    use super::*;
    use crate::cadapter::client::ObjectClient;
    use crate::cadapter::localfs::LocalFsBackend;
    use crate::chuck::chunk::ChunkLayout;
    use crate::chuck::store::ObjectBlockStore;
    use crate::fuse::mount::mount_vfs_unprivileged;
    use crate::meta::factory::create_meta_store_from_url;
    use std::fs;
    use std::io::Write;
    use std::time::Duration as StdDuration;

    // Basic Linux mount smoke test controlled by SLAYERFS_FUSE_TEST
    #[tokio::test]
    async fn smoke_mount_and_basic_ops() {
        if std::env::var("SLAYERFS_FUSE_TEST").ok().as_deref() != Some("1") {
            eprintln!("skip fuse mount test: set SLAYERFS_FUSE_TEST=1 to enable");
            return;
        }

        let layout = ChunkLayout::default();
        let tmp_data = tempfile::tempdir().expect("tmp data");
        let client = ObjectClient::new(LocalFsBackend::new(tmp_data.path()));
        let meta = create_meta_store_from_url("sqlite::memory:")
            .await
            .expect("create meta store");
        let store = ObjectBlockStore::new(client);

        let fs = VFS::new(layout, store, meta.store().clone())
            .await
            .expect("create VFS");

        // Prepare the mount point
        let mnt = tempfile::tempdir().expect("tmp mount");
        let mnt_path = mnt.path().to_path_buf();

        // Mount in the background (until unmount)
        let handle = match mount_vfs_unprivileged(fs, &mnt_path).await {
            Ok(h) => h,
            Err(e) => {
                eprintln!("skip fuse test: mount failed: {e}");
                return;
            }
        };

        // Give kernel/daemon a bit of time to finish INIT
        tokio::time::sleep(StdDuration::from_millis(2000)).await;

        // Basic directory/file operations
        let dir = mnt_path.join("a");
        fs::create_dir(&dir).expect("mkdir");
        let file_path = dir.join("hello.txt");
        {
            let mut f = fs::File::create(&file_path).expect("create file");
            f.write_all(b"abc").expect("write");
            f.flush().expect("flush");
        }
        let content = fs::read(&file_path).expect("read back");
        assert_eq!(content, b"abc");

        // List the directory
        let list = fs::read_dir(&dir)
            .expect("readdir")
            .filter_map(|e| e.ok())
            .map(|e| e.file_name())
            .collect::<Vec<_>>();
        assert!(list.iter().any(|n| n.to_string_lossy() == "hello.txt"));

        let hard_dir = mnt_path.join("hard");
        fs::create_dir(&hard_dir).expect("mkdir hard");

        let hard_a = hard_dir.join("a.txt");
        fs::write(&hard_a, b"x").expect("write hard a");
        let hard_b = hard_dir.join("b.txt");
        fs::hard_link(&hard_a, &hard_b).expect("hardlink");

        let sub_dir = hard_dir.join("sub");
        fs::create_dir(&sub_dir).expect("mkdir sub");
        let sub_file = sub_dir.join("c.txt");
        fs::write(&sub_file, b"y").expect("write sub file");

        let sub_list = fs::read_dir(&sub_dir)
            .expect("readdir sub")
            .filter_map(|e| e.ok())
            .map(|e| e.file_name())
            .collect::<Vec<_>>();
        assert!(sub_list.iter().any(|n| n.to_string_lossy() == "."));
        assert!(sub_list.iter().any(|n| n.to_string_lossy() == ".."));
        assert!(sub_list.iter().any(|n| n.to_string_lossy() == "c.txt"));

        let sub_dotdot = fs::read_link(sub_dir.join(".."));
        assert!(sub_dotdot.is_err());

        // Delete and unmount
        fs::remove_file(&hard_b).expect("unlink hard b");
        fs::remove_file(&hard_a).expect("unlink hard a");
        fs::remove_file(&sub_file).expect("unlink sub file");
        fs::remove_dir(&sub_dir).expect("rmdir sub");
        fs::remove_dir(&hard_dir).expect("rmdir hard");
        fs::remove_file(&file_path).expect("unlink");

        // Explicitly unmount and wait
        if let Err(e) = handle.unmount().await {
            eprintln!("unmount error: {e}");
        }
    }
}

impl<S, M> VFS<S, M>
where
    S: BlockStore + Send + Sync + 'static,
    M: MetaLayer + Send + Sync + 'static,
{
    async fn apply_new_entry_attrs(
        &self,
        ino: i64,
        uid: u32,
        gid: u32,
        mode: Option<u32>,
    ) -> Option<VfsFileAttr> {
        let req = SetAttrRequest {
            uid: Some(uid),
            gid: Some(gid),
            mode: mode.map(|bits| bits & 0o7777),
            ..Default::default()
        };
        if attr_request_is_empty(&req) {
            return self.stat_ino(ino).await;
        }
        match self.set_attr(ino, &req, SetAttrFlags::empty()).await {
            Ok(attr) => Some(attr),
            Err(_err) => self.stat_ino(ino).await,
        }
    }
}
#[allow(refining_impl_trait_reachable)]
impl<S, M> Filesystem for VFS<S, M>
where
    S: BlockStore + Send + Sync + 'static,
    M: MetaLayer + Send + Sync + 'static,
{
    async fn init(&self, _req: Request) -> FuseResult<ReplyInit> {
        Ok(ReplyInit::default())
    }

    async fn destroy(&self, _req: Request) {}

    // Call into VFS to resolve parent inode + name → child inode; if found, build ReplyEntry
    async fn lookup(&self, req: Request, parent: u64, name: &OsStr) -> FuseResult<ReplyEntry> {
        debug!(
            unique = req.unique,
            parent,
            name = %name.to_string_lossy(),
            "fuse.lookup"
        );
        let name_str = name.to_string_lossy();
        let child = self.child_of(parent as i64, name_str.as_ref()).await;
        let Some(child_ino) = child else {
            return Err(libc::ENOENT.into());
        };
        let Some(vattr) = self.stat_ino(child_ino).await else {
            return Err(libc::ENOENT.into());
        };
        let attr = vfs_to_fuse_attr(&vattr, &req);
        // Keep generation at 0 and set TTL to 1s (tunable)
        Ok(ReplyEntry {
            ttl: Duration::from_secs(1),
            attr,
            generation: 0,
        })
    }

    // Open file: allocate a handle for read/write operations.
    async fn open(&self, _req: Request, ino: u64, flags: u32) -> FuseResult<ReplyOpen> {
        debug!(ino, flags, "fuse.open");
        // Verify the inode exists and is a file
        let Some(attr) = self.stat_ino(ino as i64).await else {
            return Err(libc::ENOENT.into());
        };
        if matches!(attr.kind, VfsFileType::Dir) {
            return Err(libc::EISDIR.into());
        }

        let accmode = flags & (libc::O_ACCMODE as u32);
        let read = accmode != (libc::O_WRONLY as u32);
        let write = accmode != (libc::O_RDONLY as u32);
        let fh = self
            .open(ino as i64, attr.clone(), read, write)
            .await
            .map_err(Into::<Errno>::into)?;

        Ok(ReplyOpen { fh, flags })
    }

    // Open directory: create handle for caching
    async fn opendir(&self, _req: Request, ino: u64, _flags: u32) -> FuseResult<ReplyOpen> {
        debug!(ino, "fuse.opendir");
        let Some(attr) = self.stat_ino(ino as i64).await else {
            return Err(libc::ENOENT.into());
        };
        if !matches!(attr.kind, VfsFileType::Dir) {
            return Err(libc::ENOTDIR.into());
        }

        // Create directory handle for efficient readdir operations
        let fh = self
            .opendir(ino as i64)
            .await
            .map_err(Into::<Errno>::into)?;

        Ok(ReplyOpen { fh, flags: 0 })
    }

    // Read file: inode-based read
    async fn read(
        &self,
        _req: Request,
        ino: u64,
        fh: u64,
        offset: u64,
        size: u32,
    ) -> FuseResult<ReplyData> {
        debug!(ino, fh, offset, size, "fuse.read");
        // Verify inode exists
        if self.stat_ino(ino as i64).await.is_none() {
            return Err(libc::ENOENT.into());
        };

        let data = if fh != 0 {
            self.read(fh, offset, size as usize)
                .await
                .map_err(Into::<Errno>::into)?
        } else {
            let attr = self
                .stat_ino(ino as i64)
                .await
                .ok_or_else(|| Errno::from(libc::ENOENT))?;
            let tmp_fh = self
                .open(ino as i64, attr, true, false)
                .await
                .map_err(Into::<Errno>::into)?;
            let out = self
                .read(tmp_fh, offset, size as usize)
                .await
                .map_err(Into::<Errno>::into)?;
            let _ = self.close(tmp_fh).await;
            out
        };

        Ok(ReplyData {
            data: Bytes::from(data),
        })
    }

    async fn readlink(&self, _req: Request, ino: u64) -> FuseResult<ReplyData> {
        debug!(ino, "fuse.readlink");
        let target = self.readlink_ino(ino as i64).await.map_err(Errno::from)?;

        // Update atime after successful readlink
        let _ = self.update_atime(ino as i64).await;

        Ok(ReplyData {
            data: Bytes::copy_from_slice(target.as_bytes()),
        })
    }

    async fn write(
        &self,
        _req: Request,
        ino: u64,
        fh: u64,
        offset: u64,
        data: &[u8],
        _write_flags: u32,
        _flags: u32,
    ) -> FuseResult<ReplyWrite> {
        debug!(ino, fh, offset, size = data.len(), "fuse.write");
        let n = if fh != 0 {
            self.write(fh, offset, data)
                .await
                .map_err(Into::<Errno>::into)? as u32
        } else {
            self.write_ino(ino as i64, offset, data)
                .await
                .map_err(Into::<Errno>::into)? as u32
        };
        Ok(ReplyWrite { written: n })
    }

    // Ask VFS for inode attributes (flags ignored when fh is valid)
    async fn getattr(
        &self,
        req: Request,
        ino: u64,
        fh: Option<u64>,
        _flags: u32,
    ) -> FuseResult<ReplyAttr> {
        debug!(unique = req.unique, ino, fh = ?fh, "fuse.getattr");
        let vattr_opt = self.stat_ino(ino as i64).await;
        let vattr = if let Some(vattr) = vattr_opt {
            vattr
        } else if let Some(fh_value) = fh {
            let mut fallback_attr = self
                .handle_attr(fh_value)
                .ok_or_else(|| Errno::from(libc::ENOENT))?;
            fallback_attr.nlink = 0;
            fallback_attr
        } else if let Some(mut fallback_attr) = self.handle_attr_by_ino(ino as i64) {
            fallback_attr.nlink = 0;
            fallback_attr
        } else {
            return Err(libc::ENOENT.into());
        };

        let attr = vfs_to_fuse_attr(&vattr, &req);
        Ok(ReplyAttr {
            ttl: Duration::from_secs(1),
            attr,
        })
    }

    // Set attributes: delegate to metadata layer for mode/uid/gid/size/timestamps.
    // Permission checks are handled by the kernel (via default_permissions mount option).
    async fn setattr(
        &self,
        req: Request,
        ino: u64,
        _fh: Option<u64>,
        set_attr: SetAttr,
    ) -> FuseResult<ReplyAttr> {
        debug!(unique = req.unique, ino, set_attr = ?set_attr, "fuse.setattr");
        let (meta_req, meta_flags) = fuse_setattr_to_meta(&set_attr);

        // If no attributes to set, just return current attributes
        if attr_request_is_empty(&meta_req) && meta_flags.is_empty() {
            let Some(vattr) = self.stat_ino(ino as i64).await else {
                return Err(libc::ENOENT.into());
            };
            let attr = vfs_to_fuse_attr(&vattr, &req);
            return Ok(ReplyAttr {
                ttl: Duration::from_secs(1),
                attr,
            });
        }

        // Apply the attribute changes
        let vattr = self
            .set_attr(ino as i64, &meta_req, meta_flags)
            .await
            .map_err(Into::<Errno>::into)?;

        let attr = vfs_to_fuse_attr(&vattr, &req);
        Ok(ReplyAttr {
            ttl: Duration::from_secs(1),
            attr,
        })
    }

    // Call VFS to list directory and stream DirectoryEntry items (with error/offset handling)
    async fn readdir<'a>(
        &'a self,
        _req: Request,
        ino: u64,
        fh: u64,
        offset: i64,
    ) -> FuseResult<ReplyDirectory<BoxStream<'a, FuseResult<DirectoryEntry>>>> {
        debug!(ino, fh, offset, "fuse.readdir");
        // Try to use handle first
        let entries = if fh != 0 {
            let entries_offset = offset.saturating_sub(3) as u64;
            self.readdir(fh, entries_offset)
        } else {
            None
        };

        // Fallback to stateless mode if handle not found
        let entries = if let Some(e) = entries {
            e
        } else {
            // Fallback: directly read from meta layer
            let meta_entries = self.readdir_ino(ino as i64).await;
            match meta_entries {
                Some(v) => v,
                None => {
                    if self.stat_ino(ino as i64).await.is_some() {
                        return Err(libc::ENOTDIR.into());
                    } else {
                        return Err(libc::ENOENT.into());
                    }
                }
            }
        };

        // Assemble entries including '.' and '..'; offsets reference the previous entry so start at offset+1
        let mut all: Vec<DirectoryEntry> = Vec::with_capacity(entries.len() + 2);

        // Add "." and ".." entries for handle-based reads
        if fh != 0 && offset <= 0 {
            all.push(DirectoryEntry {
                inode: ino,
                kind: FuseFileType::Directory,
                name: OsString::from("."),
                offset: 1,
            });
            let parent_ino = self
                .parent_of(ino as i64)
                .await
                .unwrap_or_else(|| self.root_ino()) as u64;
            all.push(DirectoryEntry {
                inode: parent_ino,
                kind: FuseFileType::Directory,
                name: OsString::from(".."),
                offset: 2,
            });
        }

        // Actual child entries
        for (i, e) in entries.iter().enumerate() {
            all.push(DirectoryEntry {
                inode: e.ino as u64,
                kind: vfs_kind_to_fuse(e.kind),
                name: OsString::from(e.name.clone()),
                offset: (offset.max(0) as u64 + i as u64 + if fh != 0 { 3 } else { 0 }) as i64,
            });
        }

        let stream_iter = stream::iter(all.into_iter().map(Ok));
        let boxed: BoxStream<'a, FuseResult<DirectoryEntry>> = Box::pin(stream_iter);
        Ok(ReplyDirectory { entries: boxed })
    }

    // Directory read with attributes (lookup + readdir), returning DirectoryEntryPlus
    async fn readdirplus<'a>(
        &'a self,
        req: Request,
        ino: u64,
        fh: u64,
        offset: u64,
        _lock_owner: u64,
    ) -> FuseResult<ReplyDirectoryPlus<BoxStream<'a, FuseResult<DirectoryEntryPlus>>>> {
        debug!(unique = req.unique, ino, fh, offset, "fuse.readdirplus");
        let ttl = Duration::from_secs(1);
        let mut all: Vec<DirectoryEntryPlus> = Vec::new();

        // Try to use handle first
        let entries_from_handle = if fh != 0 {
            if offset == 0 {
                // Add "." entry
                if let Some(attr) = self.stat_ino(ino as i64).await {
                    let fattr = vfs_to_fuse_attr(&attr, &req);
                    all.push(DirectoryEntryPlus {
                        inode: ino,
                        generation: 0,
                        kind: FuseFileType::Directory,
                        name: OsString::from("."),
                        offset: 1,
                        attr: fattr,
                        entry_ttl: ttl,
                        attr_ttl: ttl,
                    });
                } else {
                    return Err(libc::ENOENT.into());
                }
                // Add ".." entry
                let parent_ino = self
                    .parent_of(ino as i64)
                    .await
                    .unwrap_or_else(|| self.root_ino()) as u64;
                if let Some(pattr) = self.stat_ino(parent_ino as i64).await {
                    let f = vfs_to_fuse_attr(&pattr, &req);
                    all.push(DirectoryEntryPlus {
                        inode: parent_ino,
                        generation: 0,
                        kind: FuseFileType::Directory,
                        name: OsString::from(".."),
                        offset: 2,
                        attr: f,
                        entry_ttl: ttl,
                        attr_ttl: ttl,
                    });
                }
            }

            let entries_offset = offset.saturating_sub(2);
            self.readdir(fh, entries_offset)
        } else {
            None
        };

        // Fallback to stateless mode if handle not found
        let entries = if let Some(e) = entries_from_handle {
            e
        } else {
            // Fallback: directly read from meta layer
            let meta_entries = self.readdir_ino(ino as i64).await;
            match meta_entries {
                Some(v) => v,
                None => {
                    if self.stat_ino(ino as i64).await.is_some() {
                        return Err(libc::ENOTDIR.into());
                    } else {
                        return Err(libc::ENOENT.into());
                    }
                }
            }
        };

        let entries_offset = offset.saturating_sub(2);
        for (i, e) in entries.iter().enumerate() {
            let Some(cattr) = self.stat_ino(e.ino).await else {
                continue;
            };
            let fattr = vfs_to_fuse_attr(&cattr, &req);
            all.push(DirectoryEntryPlus {
                inode: e.ino as u64,
                generation: 0,
                kind: vfs_kind_to_fuse(e.kind),
                name: OsString::from(e.name.clone()),
                offset: (entries_offset + i as u64 + 3) as i64,
                attr: fattr,
                entry_ttl: ttl,
                attr_ttl: ttl,
            });
        }

        let stream_iter = stream::iter(all.into_iter().map(Ok));
        let boxed: BoxStream<'a, FuseResult<DirectoryEntryPlus>> = Box::pin(stream_iter);
        Ok(ReplyDirectoryPlus { entries: boxed })
    }

    // Filesystem statfs: best-effort statistics from MetaStore
    async fn statfs(&self, _req: Request, _ino: u64) -> FuseResult<ReplyStatFs> {
        let bsize: u32 = 4096;
        let frsize: u32 = 4096;
        let (blocks, bfree, bavail, files, ffree) = match self.stat_fs().await {
            Ok(snapshot) => {
                let blocks = snapshot.total_space / frsize as u64;
                let bfree = snapshot.available_space / frsize as u64;
                let bavail = bfree;
                let files = snapshot
                    .used_inodes
                    .saturating_add(snapshot.available_inodes);
                let ffree = snapshot.available_inodes;
                (blocks, bfree, bavail, files, ffree)
            }
            Err(e) => {
                error!("statfs failed: {e}");
                (0, 0, 0, 0, u64::MAX)
            }
        };
        Ok(ReplyStatFs {
            blocks,
            bfree,
            bavail,
            files,
            ffree,
            bsize,
            namelen: NAME_MAX as u32,
            frsize,
        })
    }

    // Create a special file node (regular file, FIFO, etc.)
    // Note: Special files beyond regular/dir are not supported in this implementation
    async fn mknod(
        &self,
        req: Request,
        parent: u64,
        name: &OsStr,
        mode: u32,
        _rdev: u32,
    ) -> FuseResult<ReplyEntry> {
        debug!(
            unique = req.unique,
            parent,
            name = %name.to_string_lossy(),
            mode,
            "fuse.mknod"
        );
        let name = name.to_string_lossy();

        // Validate parent
        let Some(pattr) = self.stat_ino(parent as i64).await else {
            return Err(libc::ENOENT.into());
        };
        if !matches!(pattr.kind, VfsFileType::Dir) {
            return Err(libc::ENOTDIR.into());
        }

        // Check for conflicts
        if let Some(_child) = self.child_of(parent as i64, name.as_ref()).await {
            return Err(libc::EEXIST.into());
        }

        // Build the full path
        let Some(mut p) = self.path_of(parent as i64).await else {
            return Err(libc::ENOENT.into());
        };
        if p != "/" {
            p.push('/');
        }
        p.push_str(&name);

        // Extract file type from mode
        let file_type = mode & libc::S_IFMT;

        let ino = match file_type {
            libc::S_IFREG => {
                // Regular file - use create_file
                self.create_file(&p).await.map_err(Errno::from)?
            }
            libc::S_IFDIR => {
                // Directory - use mkdir_p
                self.mkdir_p(&p).await.map_err(Errno::from)?
            }
            libc::S_IFIFO | libc::S_IFSOCK | libc::S_IFCHR | libc::S_IFBLK => {
                return Err(libc::ENOSYS.into());
            }
            _ => {
                return Err(libc::EINVAL.into());
            }
        };

        // Apply mode (preserve special bits)
        let Some(vattr) = self
            .apply_new_entry_attrs(ino, req.uid, req.gid, Some(mode & 0o7777))
            .await
        else {
            return Err(libc::ENOENT.into());
        };

        let attr = vfs_to_fuse_attr(&vattr, &req);
        Ok(ReplyEntry {
            ttl: Duration::from_secs(1),
            attr,
            generation: 0,
        })
    }

    // Create a single-level directory; return EEXIST if it already exists.
    async fn mkdir(
        &self,
        req: Request,
        parent: u64,
        name: &OsStr,
        mode: u32,
        umask: u32,
    ) -> FuseResult<ReplyEntry> {
        debug!(
            unique = req.unique,
            parent,
            name = %name.to_string_lossy(),
            mode,
            umask,
            "fuse.mkdir"
        );
        let name = name.to_string_lossy();
        // Parent must be a directory
        let Some(pattr) = self.stat_ino(parent as i64).await else {
            return Err(libc::ENOENT.into());
        };
        if !matches!(pattr.kind, VfsFileType::Dir) {
            return Err(libc::ENOTDIR.into());
        }
        // Check for conflicts
        if let Some(_child) = self.child_of(parent as i64, name.as_ref()).await {
            return Err(libc::EEXIST.into());
        }
        // Build the path and create
        let Some(mut p) = self.path_of(parent as i64).await else {
            return Err(libc::ENOENT.into());
        };
        if p != "/" {
            p.push('/');
        }
        p.push_str(&name);
        let _ino = self.mkdir_p(&p).await.map_err(Errno::from)?;
        // Preserve special bits (sticky, setuid, setgid) along with permission bits
        let masked_mode = (mode & 0o7777) & !(umask & 0o777);
        let Some(vattr) = self
            .apply_new_entry_attrs(_ino, req.uid, req.gid, Some(masked_mode))
            .await
        else {
            return Err(libc::ENOENT.into());
        };
        let attr = vfs_to_fuse_attr(&vattr, &req);
        Ok(ReplyEntry {
            ttl: Duration::from_secs(1),
            attr,
            generation: 0,
        })
    }

    // Create and open a file
    async fn create(
        &self,
        req: Request,
        parent: u64,
        name: &OsStr,
        mode: u32,
        flags: u32,
    ) -> FuseResult<ReplyCreated> {
        debug!(
            unique = req.unique,
            parent,
            name = %name.to_string_lossy(),
            mode,
            flags,
            "fuse.create"
        );
        let name = name.to_string_lossy();
        // Validate parent
        let Some(pattr) = self.stat_ino(parent as i64).await else {
            return Err(libc::ENOENT.into());
        };
        if !matches!(pattr.kind, VfsFileType::Dir) {
            return Err(libc::ENOTDIR.into());
        }
        let Some(mut p) = self.path_of(parent as i64).await else {
            return Err(libc::ENOENT.into());
        };
        if p != "/" {
            p.push('/');
        }
        p.push_str(&name);
        let ino = self.create_file(&p).await.map_err(Errno::from)?;
        let Some(vattr) = self
            .apply_new_entry_attrs(ino, req.uid, req.gid, Some(mode & 0o7777))
            .await
        else {
            return Err(libc::ENOENT.into());
        };
        let attr = vfs_to_fuse_attr(&vattr, &req);

        let accmode = flags & (libc::O_ACCMODE as u32);
        let read = accmode != (libc::O_WRONLY as u32);
        let write = accmode != (libc::O_RDONLY as u32);
        let fh = self
            .open(ino, vattr.clone(), read, write)
            .await
            .map_err(Into::<Errno>::into)?;

        Ok(ReplyCreated {
            ttl: Duration::from_secs(1),
            attr,
            generation: 0,
            fh,
            flags: 0,
        })
    }

    async fn link(
        &self,
        req: Request,
        ino: u64,
        new_parent: u64,
        new_name: &OsStr,
    ) -> FuseResult<ReplyEntry> {
        debug!(
            unique = req.unique,
            ino,
            new_parent,
            new_name = %new_name.to_string_lossy(),
            "fuse.link"
        );
        let Some(existing_attr) = self.stat_ino(ino as i64).await else {
            return Err(libc::ENOENT.into());
        };
        if matches!(existing_attr.kind, VfsFileType::Dir) {
            return Err(libc::EISDIR.into());
        }

        let Some(parent_attr) = self.stat_ino(new_parent as i64).await else {
            return Err(libc::ENOENT.into());
        };
        if !matches!(parent_attr.kind, VfsFileType::Dir) {
            return Err(libc::ENOTDIR.into());
        }

        let new_name_str = new_name.to_string_lossy();

        if self
            .child_of(new_parent as i64, new_name_str.as_ref())
            .await
            .is_some()
        {
            return Err(libc::EEXIST.into());
        }

        let Some(mut parent_path) = self.path_of(new_parent as i64).await else {
            return Err(libc::ENOENT.into());
        };
        if parent_path != "/" {
            parent_path.push('/');
        }
        if new_name_str.is_empty() {
            return Err(libc::EINVAL.into());
        }
        parent_path.push_str(new_name_str.as_ref());

        let Some(existing_path) = self.path_of(ino as i64).await else {
            return Err(libc::ENOENT.into());
        };

        let attr = VFS::link(self, &existing_path, &parent_path)
            .await
            .map_err(Errno::from)?;

        let fuse_attr = vfs_to_fuse_attr(&attr, &req);
        Ok(ReplyEntry {
            ttl: Duration::from_secs(1),
            attr: fuse_attr,
            generation: 0,
        })
    }

    async fn symlink(
        &self,
        req: Request,
        parent: u64,
        name: &OsStr,
        link: &OsStr,
    ) -> FuseResult<ReplyEntry> {
        debug!(
            unique = req.unique,
            parent,
            name = %name.to_string_lossy(),
            link = %link.to_string_lossy(),
            "fuse.symlink"
        );
        let name = name.to_string_lossy();
        if name.is_empty() {
            return Err(libc::EINVAL.into());
        }

        let Some(pattr) = self.stat_ino(parent as i64).await else {
            return Err(libc::ENOENT.into());
        };
        if !matches!(pattr.kind, VfsFileType::Dir) {
            return Err(libc::ENOTDIR.into());
        }

        if self.child_of(parent as i64, name.as_ref()).await.is_some() {
            return Err(libc::EEXIST.into());
        }

        let Some(mut parent_path) = self.path_of(parent as i64).await else {
            return Err(libc::ENOENT.into());
        };
        if parent_path != "/" {
            parent_path.push('/');
        }
        parent_path.push_str(&name);

        let target = link.to_string_lossy();

        let (ino, vattr) = self
            .create_symlink(&parent_path, target.as_ref())
            .await
            .map_err(Errno::from)?;

        let attr = self
            .apply_new_entry_attrs(ino, req.uid, req.gid, None)
            .await
            .unwrap_or(vattr);

        Ok(ReplyEntry {
            ttl: Duration::from_secs(1),
            attr: vfs_to_fuse_attr(&attr, &req),
            generation: 0,
        })
    }

    // Remove a file
    async fn unlink(&self, _req: Request, parent: u64, name: &OsStr) -> FuseResult<()> {
        debug!(parent, name = %name.to_string_lossy(), "fuse.unlink");
        let name = name.to_string_lossy();
        // Ensure parent directory exists and has the right type
        let Some(pattr) = self.stat_ino(parent as i64).await else {
            return Err(libc::ENOENT.into());
        };
        if !matches!(pattr.kind, VfsFileType::Dir) {
            return Err(libc::ENOTDIR.into());
        }
        // Target must exist and be a file
        let Some(child) = self.child_of(parent as i64, name.as_ref()).await else {
            return Err(libc::ENOENT.into());
        };
        let Some(cattr) = self.stat_ino(child).await else {
            return Err(libc::ENOENT.into());
        };
        if matches!(cattr.kind, VfsFileType::Dir) {
            return Err(libc::EISDIR.into());
        }
        let Some(mut p) = self.path_of(parent as i64).await else {
            return Err(libc::ENOENT.into());
        };
        if p != "/" {
            p.push('/');
        }
        p.push_str(&name);
        self.unlink(&p).await.map_err(Errno::from)
    }

    // Remove an empty directory
    async fn rmdir(&self, _req: Request, parent: u64, name: &OsStr) -> FuseResult<()> {
        debug!(parent, name = %name.to_string_lossy(), "fuse.rmdir");
        let name = name.to_string_lossy();
        let Some(pattr) = self.stat_ino(parent as i64).await else {
            return Err(libc::ENOENT.into());
        };
        if !matches!(pattr.kind, VfsFileType::Dir) {
            return Err(libc::ENOTDIR.into());
        }
        // Target must be a directory
        let Some(child) = self.child_of(parent as i64, name.as_ref()).await else {
            return Err(libc::ENOENT.into());
        };
        let Some(cattr) = self.stat_ino(child).await else {
            return Err(libc::ENOENT.into());
        };
        if !matches!(cattr.kind, VfsFileType::Dir) {
            return Err(libc::ENOTDIR.into());
        }
        let Some(mut p) = self.path_of(parent as i64).await else {
            return Err(libc::ENOENT.into());
        };
        if p != "/" {
            p.push('/');
        }
        p.push_str(&name);
        self.rmdir(&p).await.map_err(Errno::from)
    }

    // Rename (files or directories)
    async fn rename(
        &self,
        _req: Request,
        parent: u64,
        name: &OsStr,
        new_parent: u64,
        new_name: &OsStr,
    ) -> FuseResult<()> {
        debug!(
            parent,
            name = %name.to_string_lossy(),
            new_parent,
            new_name = %new_name.to_string_lossy(),
            "fuse.rename"
        );
        let name = name.to_string_lossy();
        let new_name = new_name.to_string_lossy();

        // Validate input parameters
        if name.is_empty() || new_name.is_empty() {
            return Err(libc::EINVAL.into());
        }

        // Check for invalid characters in names
        if name.contains('/')
            || name.contains('\0')
            || new_name.contains('/')
            || new_name.contains('\0')
        {
            return Err(libc::EINVAL.into());
        }

        // Prevent renaming to the same location
        if parent == new_parent && name == new_name {
            return Err(libc::EINVAL.into());
        }

        // Ensure the source exists
        let Some(src_ino) = self.child_of(parent as i64, name.as_ref()).await else {
            return Err(libc::ENOENT.into());
        };

        // Get source attributes for validation
        let Some(src_attr) = self.stat_ino(src_ino).await else {
            return Err(libc::ENOENT.into());
        };

        // Validate the destination parent
        let Some(pattr) = self.stat_ino(new_parent as i64).await else {
            return Err(libc::ENOENT.into());
        };
        if !matches!(pattr.kind, VfsFileType::Dir) {
            return Err(libc::ENOTDIR.into());
        }

        // Build full paths and perform the rename
        let Some(mut oldp) = self.path_of(parent as i64).await else {
            return Err(libc::ENOENT.into());
        };
        if oldp != "/" {
            oldp.push('/');
        }
        oldp.push_str(&name);

        let Some(mut newp) = self.path_of(new_parent as i64).await else {
            return Err(libc::ENOENT.into());
        };
        if newp != "/" {
            newp.push('/');
        }
        newp.push_str(&new_name);

        // Check for circular rename at FUSE level
        if newp.starts_with(&(oldp.clone() + "/")) && matches!(src_attr.kind, VfsFileType::Dir) {
            return Err(libc::EINVAL.into());
        }

        VFS::rename(self, &oldp, &newp).await.map_err(|e| {
            match e {
                VfsError::NotFound { .. } => libc::ENOENT,
                VfsError::AlreadyExists { .. } => libc::EEXIST,
                VfsError::NotADirectory { .. } => libc::ENOTDIR,
                VfsError::IsADirectory { .. } => libc::EISDIR,
                VfsError::DirectoryNotEmpty { .. } => libc::ENOTEMPTY,
                VfsError::PermissionDenied { .. } => libc::EACCES,
                VfsError::CircularRename { .. } => libc::EINVAL,
                VfsError::InvalidRenameTarget { .. } => libc::EINVAL,
                VfsError::CrossesDevices => libc::EXDEV,
                _ => libc::EIO,
            }
            .into()
        })
    }

    // ===== Resource release & sync: stateless implementation, return success =====
    // Close file handle
    async fn release(
        &self,
        _req: Request,
        _inode: u64,
        fh: u64,
        _flags: u32,
        _lock_owner: u64,
        _flush: bool,
    ) -> FuseResult<()> {
        debug!(fh, "fuse.release");
        let _ = self.close(fh).await;
        Ok(())
    }

    // Flush file (close path callback)
    async fn flush(&self, _req: Request, _inode: u64, fh: u64, _lock_owner: u64) -> FuseResult<()> {
        debug!(fh, "fuse.flush");
        self.flush(fh).await.map_err(Errno::from)
    }

    // Sync file content to backend
    async fn fsync(&self, _req: Request, _inode: u64, fh: u64, datasync: bool) -> FuseResult<()> {
        debug!(fh, datasync, "fuse.fsync");
        self.fsync(fh, datasync).await.map_err(Errno::from)
    }

    async fn setxattr(
        &self,
        _req: Request,
        inode: u64,
        name: &OsStr,
        value: &[u8],
        flags: u32,
        position: u32,
    ) -> FuseResult<()> {
        if position != 0 {
            return Err(libc::EINVAL.into());
        }
        if self.stat_ino(inode as i64).await.is_none() {
            return Err(libc::ENOENT.into());
        }
        let name = name.to_string_lossy();
        self.set_xattr_ino(inode as i64, &name, value, flags)
            .await
            .map_err(|e| match e {
                MetaError::AlreadyExists { .. } => Errno::from(libc::EEXIST),
                MetaError::NotSupported(_) | MetaError::NotImplemented => Errno::from(libc::ENOSYS),
                MetaError::NotFound(_) => Errno::from(libc::ENODATA),
                _ => Errno::from(libc::EIO),
            })
    }

    async fn getxattr(
        &self,
        _req: Request,
        inode: u64,
        name: &OsStr,
        size: u32,
    ) -> FuseResult<ReplyXAttr> {
        if self.stat_ino(inode as i64).await.is_none() {
            return Err(libc::ENOENT.into());
        }
        let name = name.to_string_lossy();
        let value = self
            .get_xattr_ino(inode as i64, &name)
            .await
            .map_err(|e| match e {
                MetaError::NotSupported(_) | MetaError::NotImplemented => Errno::from(libc::ENOSYS),
                _ => Errno::from(libc::EIO),
            })?
            .ok_or_else(|| Errno::from(libc::ENODATA))?;
        if size == 0 {
            return Ok(ReplyXAttr::Size(value.len() as u32));
        }
        if (size as usize) < value.len() {
            return Err(libc::ERANGE.into());
        }
        Ok(ReplyXAttr::Data(Bytes::from(value)))
    }

    async fn listxattr(&self, _req: Request, inode: u64, size: u32) -> FuseResult<ReplyXAttr> {
        if self.stat_ino(inode as i64).await.is_none() {
            return Err(libc::ENOENT.into());
        }
        let names = self
            .list_xattr_ino(inode as i64)
            .await
            .map_err(|e| match e {
                MetaError::NotSupported(_) | MetaError::NotImplemented => Errno::from(libc::ENOSYS),
                _ => Errno::from(libc::EIO),
            })?;
        let total_len: usize = names.iter().map(|n| n.len() + 1).sum();
        if size == 0 {
            return Ok(ReplyXAttr::Size(total_len as u32));
        }
        if (size as usize) < total_len {
            return Err(libc::ERANGE.into());
        }
        let mut data = Vec::with_capacity(total_len);
        for name in names {
            data.extend_from_slice(name.as_bytes());
            data.push(0);
        }
        Ok(ReplyXAttr::Data(Bytes::from(data)))
    }

    async fn removexattr(&self, _req: Request, inode: u64, name: &OsStr) -> FuseResult<()> {
        if self.stat_ino(inode as i64).await.is_none() {
            return Err(libc::ENOENT.into());
        }
        let name = name.to_string_lossy();
        self.remove_xattr_ino(inode as i64, &name)
            .await
            .map_err(|e| match e {
                MetaError::NotSupported(_) | MetaError::NotImplemented => libc::ENOSYS.into(),
                MetaError::NotFound(_) => libc::ENODATA.into(),
                _ => libc::EIO.into(),
            })
    }

    // Close directory handle
    async fn releasedir(&self, _req: Request, _inode: u64, fh: u64, _flags: u32) -> FuseResult<()> {
        debug!(fh, "fuse.releasedir");
        if fh == 0 {
            return Ok(()); // No handle to release
        }

        if let Err(e) = self.closedir(fh) {
            match e {
                VfsError::StaleNetworkFileHandle => {
                    // Handle not found, but that's ok - might be a stateless readdir
                    debug!("releasedir: handle {} not found (stateless mode)", fh);
                }
                _ => {
                    error!("Error releasing directory handle {}: {:?}", fh, e);
                    return Err(libc::EIO.into());
                }
            }
        }
        Ok(())
    }

    // Sync directory content to backend
    async fn fsyncdir(
        &self,
        _req: Request,
        _inode: u64,
        _fh: u64,
        _datasync: bool,
    ) -> FuseResult<()> {
        Ok(())
    }

    // Test for a POSIX file lock
    async fn getlk(
        &self,
        _req: Request,
        inode: u64,
        _fh: u64,
        lock_owner: u64,
        start: u64,
        end: u64,
        lock_type: u32,
        _pid: u32,
    ) -> FuseResult<ReplyLock> {
        debug!(inode, lock_owner, start, end, lock_type, "fuse.getlk");
        // Convert FUSE lock type to our internal type
        let fl_type = match lock_type as i32 {
            libc::F_RDLCK => FileLockType::Read,
            libc::F_WRLCK => FileLockType::Write,
            libc::F_UNLCK => FileLockType::UnLock,
            _ => return Err(libc::EINVAL.into()),
        };

        let query = FileLockQuery {
            owner: lock_owner as i64,
            lock_type: fl_type,
            range: FileLockRange { start, end },
        };

        match self.get_plock_ino(inode as i64, &query).await {
            Ok(info) => {
                // Convert internal lock type back to FUSE type
                let fuse_type = match info.lock_type {
                    FileLockType::Read => libc::F_RDLCK,
                    FileLockType::Write => libc::F_WRLCK,
                    FileLockType::UnLock => libc::F_UNLCK,
                };
                Ok(ReplyLock {
                    r#type: fuse_type as u32,
                    start: info.range.start,
                    end: info.range.end,
                    pid: info.pid,
                })
            }
            Err(e) => Err(Errno::from(e)),
        }
    }

    // Acquire, modify or release a POSIX file lock
    async fn setlk(
        &self,
        _req: Request,
        inode: u64,
        _fh: u64,
        lock_owner: u64,
        start: u64,
        end: u64,
        lock_type: u32,
        pid: u32,
        block: bool,
    ) -> FuseResult<()> {
        debug!(
            inode,
            lock_owner, start, end, lock_type, pid, block, "fuse.setlk"
        );
        // Convert FUSE lock type to our internal type
        let fl_type = match lock_type as i32 {
            libc::F_RDLCK => FileLockType::Read,
            libc::F_WRLCK => FileLockType::Write,
            libc::F_UNLCK => FileLockType::UnLock,
            _ => return Err(libc::EINVAL.into()),
        };

        let range = FileLockRange { start, end };

        // Forward block parameter to MetaStore; backend may choose to block or return conflicts
        match self
            .set_plock_ino(inode as i64, lock_owner as i64, block, fl_type, range, pid)
            .await
        {
            Ok(()) => Ok(()),
            Err(e) => Err(Errno::from(e)),
        }
    }

    // Forget (kernel reference drop); no inode ref tracking yet so no-op
    async fn forget(&self, _req: Request, _inode: u64, _nlookup: u64) {}

    // Batch forget; no-op
    async fn batch_forget(&self, _req: Request, _inodes: &[(u64, u64)]) {}

    // Interrupt an in-flight request (no tracking), so no-op
    async fn interrupt(&self, _req: Request, _unique: u64) -> FuseResult<()> {
        Ok(())
    }

    // Check file access permissions
    async fn access(&self, req: Request, ino: u64, mask: u32) -> FuseResult<()> {
        debug!(
            unique = req.unique,
            ino,
            mask,
            uid = req.uid,
            gid = req.gid,
            "fuse.access"
        );
        let Some(attr) = self.stat_ino(ino as i64).await else {
            return Err(libc::ENOENT.into());
        };

        // F_OK (0) just checks for existence
        if mask == 0 {
            return Ok(());
        }

        // Check if the requesting user has the required access
        let uid = req.uid;
        let gid = req.gid;

        // Root can access everything (except execute on non-executable files)
        if uid == 0 {
            // Root still needs execute permission to be set somewhere
            if (mask & libc::X_OK as u32) != 0 && (attr.mode & 0o111) == 0 {
                return Err(libc::EACCES.into());
            }
            return Ok(());
        }

        // Determine which permission bits to check
        let mode = if uid == attr.uid {
            // Owner permissions
            (attr.mode >> 6) & 0o7
        } else if gid == attr.gid {
            // Group permissions
            (attr.mode >> 3) & 0o7
        } else {
            // Other permissions
            attr.mode & 0o7
        };

        // Check if the requested access is allowed
        // mask uses libc constants: F_OK=0, X_OK=1, W_OK=2, R_OK=4
        if (mask & libc::R_OK as u32) != 0 && (mode & 0o4) == 0 {
            return Err(libc::EACCES.into());
        }
        if (mask & libc::W_OK as u32) != 0 && (mode & 0o2) == 0 {
            return Err(libc::EACCES.into());
        }
        if (mask & libc::X_OK as u32) != 0 && (mode & 0o1) == 0 {
            return Err(libc::EACCES.into());
        }

        Ok(())
    }
}

// =============== helpers ===============
impl From<MetaError> for Errno {
    fn from(val: MetaError) -> Self {
        let code = match val {
            MetaError::NotFound(_) => libc::ENOENT,
            MetaError::ParentNotFound(_) => libc::ENOENT,
            MetaError::NotDirectory(_) => libc::ENOTDIR,
            MetaError::DirectoryNotEmpty(_) => libc::ENOTEMPTY,
            MetaError::AlreadyExists { .. } => libc::EEXIST,
            MetaError::NotSupported(_) | MetaError::NotImplemented => libc::ENOSYS,
            MetaError::InvalidPath(_) => libc::EINVAL,
            _ => libc::EIO,
        };
        Errno::from(code)
    }
}

impl From<VfsError> for Errno {
    fn from(val: VfsError) -> Self {
        let code = match val {
            VfsError::NotFound { .. } => libc::ENOENT,
            VfsError::AlreadyExists { .. } => libc::EEXIST,
            VfsError::NotADirectory { .. } => libc::ENOTDIR,
            VfsError::IsADirectory { .. } => libc::EISDIR,
            VfsError::DirectoryNotEmpty { .. } => libc::ENOTEMPTY,
            VfsError::PermissionDenied { .. } => libc::EACCES,
            VfsError::ReadOnlyFilesystem { .. } => libc::EROFS,
            VfsError::ConnectionRefused => libc::ECONNREFUSED,
            VfsError::ConnectionReset => libc::ECONNRESET,
            VfsError::HostUnreachable => libc::EHOSTUNREACH,
            VfsError::NetworkUnreachable => libc::ENETUNREACH,
            VfsError::ConnectionAborted => libc::ECONNABORTED,
            VfsError::NotConnected => libc::ENOTCONN,
            VfsError::AddrInUse => libc::EADDRINUSE,
            VfsError::AddrNotAvailable => libc::EADDRNOTAVAIL,
            VfsError::NetworkDown => libc::ENETDOWN,
            VfsError::BrokenPipe => libc::EPIPE,
            VfsError::WouldBlock => libc::EAGAIN,
            VfsError::InvalidInput => libc::EINVAL,
            VfsError::InvalidData => libc::EINVAL,
            VfsError::TimedOut => libc::ETIMEDOUT,
            VfsError::WriteZero => libc::EIO,
            VfsError::StorageFull => libc::ENOSPC,
            VfsError::NotSeekable => libc::ESPIPE,
            VfsError::QuotaExceeded => libc::EDQUOT,
            VfsError::FileTooLarge => libc::EFBIG,
            VfsError::ResourceBusy => libc::EBUSY,
            VfsError::ExecutableFileBusy => libc::ETXTBSY,
            VfsError::Deadlock => libc::EDEADLK,
            VfsError::CrossesDevices => libc::EXDEV,
            VfsError::TooManyLinks => libc::EMLINK,
            VfsError::InvalidFilename => libc::EINVAL,
            VfsError::ArgumentListTooLong => libc::E2BIG,
            VfsError::Interrupted => libc::EINTR,
            VfsError::Unsupported => libc::ENOSYS,
            VfsError::UnexpectedEof => libc::EIO,
            VfsError::OutOfMemory => libc::ENOMEM,
            VfsError::StaleNetworkFileHandle => libc::ESTALE,
            _ => libc::EIO,
        };
        code.into()
    }
}

fn vfs_kind_to_fuse(k: VfsFileType) -> FuseFileType {
    match k {
        VfsFileType::Dir => FuseFileType::Directory,
        VfsFileType::File => FuseFileType::RegularFile,
        VfsFileType::Symlink => FuseFileType::Symlink,
    }
}

fn vfs_to_fuse_attr(v: &VfsFileAttr, _req: &Request) -> rfuse3::raw::reply::FileAttr {
    let perm = (v.mode & 0o7777) as u16;
    let blocks = v.size.div_ceil(512);
    let atime = nanos_to_timestamp(v.atime);
    let mtime = nanos_to_timestamp(v.mtime);
    let ctime = nanos_to_timestamp(v.ctime);
    rfuse3::raw::reply::FileAttr {
        ino: v.ino as u64,
        size: v.size,
        blocks,
        atime,
        mtime,
        ctime,
        #[cfg(target_os = "macos")]
        crtime: ctime,
        kind: vfs_kind_to_fuse(v.kind),
        perm,
        nlink: v.nlink,
        uid: v.uid,
        gid: v.gid,
        rdev: 0,
        #[cfg(target_os = "macos")]
        flags: 0,
        blksize: 4096,
    }
}

const NANOS_PER_SEC: i64 = 1_000_000_000;

fn nanos_to_timestamp(value: i64) -> Timestamp {
    let sec = value.div_euclid(NANOS_PER_SEC);
    let nsec = value.rem_euclid(NANOS_PER_SEC) as u32;
    Timestamp::new(sec, nsec)
}

fn timestamp_to_nanos(ts: Timestamp) -> i64 {
    ts.sec
        .saturating_mul(NANOS_PER_SEC)
        .saturating_add(ts.nsec as i64)
}
fn fuse_setattr_to_meta(set_attr: &SetAttr) -> (SetAttrRequest, SetAttrFlags) {
    let mut req = SetAttrRequest::default();
    let flags = SetAttrFlags::empty();
    if let Some(mode) = set_attr.mode {
        req.mode = Some(mode);
    }
    if let Some(uid) = set_attr.uid {
        req.uid = Some(uid);
    }
    if let Some(gid) = set_attr.gid {
        req.gid = Some(gid);
    }
    if let Some(size) = set_attr.size {
        req.size = Some(size);
    }
    if let Some(atime) = set_attr.atime {
        req.atime = Some(timestamp_to_nanos(atime));
    }
    if let Some(mtime) = set_attr.mtime {
        req.mtime = Some(timestamp_to_nanos(mtime));
    }
    if let Some(ctime) = set_attr.ctime {
        req.ctime = Some(timestamp_to_nanos(ctime));
    }
    (req, flags)
}

fn attr_request_is_empty(req: &SetAttrRequest) -> bool {
    req.mode.is_none()
        && req.uid.is_none()
        && req.gid.is_none()
        && req.size.is_none()
        && req.atime.is_none()
        && req.mtime.is_none()
        && req.ctime.is_none()
        && req.flags.is_none()
}
