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
pub mod adapter;
pub mod mount;
use crate::chuck::store::BlockStore;
use crate::meta::MetaStore;
use crate::meta::store::MetaError;
use crate::vfs::fs::{FileAttr as VfsFileAttr, FileType as VfsFileType, VFS};
use bytes::Bytes;
use rfuse3::Errno;
use rfuse3::Result as FuseResult;
use rfuse3::raw::Request;
use rfuse3::raw::reply::{
    DirectoryEntry, DirectoryEntryPlus, ReplyAttr, ReplyCreated, ReplyData, ReplyDirectory,
    ReplyDirectoryPlus, ReplyEntry, ReplyInit, ReplyOpen, ReplyStatFs, ReplyWrite,
};
use std::ffi::{OsStr, OsString};
use std::num::NonZeroU32;
use std::time::{Duration, SystemTime};

use futures_util::stream::{self, BoxStream};
use rfuse3::raw::Filesystem;
use rfuse3::{FileType as FuseFileType, SetAttr, Timestamp};

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

        // Delete and unmount
        fs::remove_file(&file_path).expect("unlink");

        // Explicitly unmount and wait
        if let Err(e) = handle.unmount().await {
            eprintln!("unmount error: {e}");
        }
    }
}
#[allow(refining_impl_trait_reachable)]
impl<S, M> Filesystem for VFS<S, M>
where
    S: BlockStore + Send + Sync + 'static,
    M: MetaStore + Send + Sync + 'static,
{
    async fn init(&self, _req: Request) -> FuseResult<ReplyInit> {
        // Use a conservative max write size (1 MiB). Tune per backend or make configurable.
        let max_write = NonZeroU32::new(1024 * 1024).unwrap();
        Ok(ReplyInit { max_write })
    }

    async fn destroy(&self, _req: Request) {}

    // Call into VFS to resolve parent inode + name → child inode; if found, build ReplyEntry
    async fn lookup(&self, req: Request, parent: u64, name: &OsStr) -> FuseResult<ReplyEntry> {
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

    // Open file: stateless IO, always return fh=0
    async fn open(&self, _req: Request, ino: u64, _flags: u32) -> FuseResult<ReplyOpen> {
        // Verify the inode exists and is a file
        let Some(attr) = self.stat_ino(ino as i64).await else {
            return Err(libc::ENOENT.into());
        };
        if matches!(attr.kind, VfsFileType::Dir) {
            return Err(libc::EISDIR.into());
        }
        Ok(ReplyOpen { fh: 0, flags: 0 })
    }

    // Open directory: allocate handle and pre-load entries
    async fn opendir(&self, _req: Request, ino: u64, _flags: u32) -> FuseResult<ReplyOpen> {
        let fh = self.opendir_handle(ino as i64).await.map_err(|e| match e {
            MetaError::NotFound(_) => libc::ENOENT,
            MetaError::NotDirectory(_) => libc::ENOTDIR,
            _ => libc::EIO,
        })?;
        Ok(ReplyOpen { fh, flags: 0 })
    }

    // Read file: map to VFS::read (path derived from inode)
    async fn read(
        &self,
        _req: Request,
        ino: u64,
        _fh: u64,
        offset: u64,
        size: u32,
    ) -> FuseResult<ReplyData> {
        // Verify inode exists
        if self.stat_ino(ino as i64).await.is_none() {
            return Err(libc::ENOENT.into());
        };

        let data = self
            .read_ino(ino as i64, offset, size as usize)
            .await
            .map_err(|_| libc::EIO)?;
        Ok(ReplyData {
            data: Bytes::from(data),
        })
    }

    async fn readlink(&self, _req: Request, ino: u64) -> FuseResult<ReplyData> {
        let target = self.readlink_ino(ino as i64).await.map_err(|e| -> Errno {
            let code = match e.as_str() {
                "not found" => libc::ENOENT,
                "not a symlink" => libc::EINVAL,
                "not supported" => libc::ENOSYS,
                _ if e.contains("not supported") => libc::ENOSYS,
                _ => libc::EIO,
            };
            code.into()
        })?;

        Ok(ReplyData {
            data: Bytes::copy_from_slice(target.as_bytes()),
        })
    }

    // Write file: map to VFS::write (path derived from inode)
    async fn write(
        &self,
        _req: Request,
        ino: u64,
        _fh: u64,
        offset: u64,
        data: &[u8],
        _write_flags: u32,
        _flags: u32,
    ) -> FuseResult<ReplyWrite> {
        let Some(path) = self.path_of(ino as i64).await else {
            return Err(libc::ENOENT.into());
        };
        let n = self
            .write(&path, offset, data)
            .await
            .map_err(|_| libc::EIO)? as u32;
        Ok(ReplyWrite { written: n })
    }

    // Ask VFS for inode attributes (flags ignored when fh is valid)
    async fn getattr(
        &self,
        req: Request,
        ino: u64,
        _fh: Option<u64>,
        _flags: u32,
    ) -> FuseResult<ReplyAttr> {
        let Some(vattr) = self.stat_ino(ino as i64).await else {
            return Err(libc::ENOENT.into());
        };
        let attr = vfs_to_fuse_attr(&vattr, &req);
        Ok(ReplyAttr {
            ttl: Duration::from_secs(1),
            attr,
        })
    }

    // Set attributes: currently only size (truncate) is supported
    async fn setattr(
        &self,
        req: Request,
        ino: u64,
        _fh: Option<u64>,
        set_attr: SetAttr,
    ) -> FuseResult<ReplyAttr> {
        if let Some(size) = set_attr.size {
            let Some(path) = self.path_of(ino as i64).await else {
                return Err(libc::ENOENT.into());
            };
            self.truncate(&path, size).await.map_err(|_| libc::EIO)?;
        }
        // Return the refreshed attributes
        let Some(vattr) = self.stat_ino(ino as i64).await else {
            return Err(libc::ENOENT.into());
        };
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
        let mut all: Vec<DirectoryEntry> = Vec::new();

        // Try to use handle first
        let entries_from_handle = if fh != 0 {
            // offset 0-2 are reserved for . and ..
            if offset == 0 {
                all.push(DirectoryEntry {
                    inode: ino,
                    kind: FuseFileType::Directory,
                    name: OsString::from("."),
                    offset: 1,
                });
                let parent_ino = self.parent_of(ino as i64).await.unwrap_or(self.root_ino()) as u64;
                all.push(DirectoryEntry {
                    inode: parent_ino,
                    kind: FuseFileType::Directory,
                    name: OsString::from(".."),
                    offset: 2,
                });
            }

            let entries_offset = if offset <= 2 { 0 } else { (offset - 2) as u64 };
            self.readdir_by_handle(fh, entries_offset).await
        } else {
            None
        };

        // Fallback to stateless mode if handle not found
        let entries = if let Some(e) = entries_from_handle {
            e
        } else {
            // Fallback: directly read from meta layer
            match self.readdir_ino(ino as i64).await {
                Some(v) => {
                    if offset == 0 {
                        all.push(DirectoryEntry {
                            inode: ino,
                            kind: FuseFileType::Directory,
                            name: OsString::from("."),
                            offset: 1,
                        });
                        let parent_ino =
                            self.parent_of(ino as i64).await.unwrap_or(self.root_ino()) as u64;
                        all.push(DirectoryEntry {
                            inode: parent_ino,
                            kind: FuseFileType::Directory,
                            name: OsString::from(".."),
                            offset: 2,
                        });
                    }
                    v
                }
                None => {
                    if self.stat_ino(ino as i64).await.is_some() {
                        return Err(libc::ENOTDIR.into());
                    } else {
                        return Err(libc::ENOENT.into());
                    }
                }
            }
        };

        let entries_offset = if offset <= 2 { 0 } else { (offset - 2) as u64 };
        for (i, e) in entries.iter().enumerate() {
            all.push(DirectoryEntry {
                inode: e.ino as u64,
                kind: vfs_kind_to_fuse(e.kind),
                name: OsString::from(e.name.clone()),
                offset: (entries_offset + i as u64 + 3) as i64,
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
                let parent_ino = self.parent_of(ino as i64).await.unwrap_or(self.root_ino()) as u64;
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
            self.readdir_by_handle(fh, entries_offset).await
        } else {
            None
        };

        // Fallback to stateless mode if handle not found
        let entries = if let Some(e) = entries_from_handle {
            e
        } else {
            // Fallback: directly read from meta layer
            if offset == 0 {
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
                let parent_ino = self.parent_of(ino as i64).await.unwrap_or(self.root_ino()) as u64;
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

            match self.readdir_ino(ino as i64).await {
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

    // Filesystem statfs: return conservative placeholder values
    async fn statfs(&self, _req: Request, _ino: u64) -> FuseResult<ReplyStatFs> {
        // Can't safely inspect internals here, so return conservative constants; can be hooked up later.
        let bsize: u32 = 4096;
        let frsize: u32 = 4096;
        let files: u64 = 0;
        let ffree: u64 = u64::MAX;
        // blocks/bfree/bavail unknown → return 0; namelen capped at 255.
        Ok(ReplyStatFs {
            blocks: 0,
            bfree: 0,
            bavail: 0,
            files,
            ffree,
            bsize,
            namelen: 255,
            frsize,
        })
    }

    // Create a single-level directory; return EEXIST if it already exists.
    async fn mkdir(
        &self,
        req: Request,
        parent: u64,
        name: &OsStr,
        _mode: u32,
        _umask: u32,
    ) -> FuseResult<ReplyEntry> {
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
        let _ino = self.mkdir_p(&p).await.map_err(|_| libc::EIO)?;
        let Some(vattr) = self.stat_ino(_ino).await else {
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
        _mode: u32,
        _flags: u32,
    ) -> FuseResult<ReplyCreated> {
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
        let ino = self.create_file(&p).await.map_err(|e| match e.as_str() {
            "is a directory" => libc::EISDIR,
            _ => libc::EIO,
        })?;
        let Some(vattr) = self.stat_ino(ino).await else {
            return Err(libc::ENOENT.into());
        };
        let attr = vfs_to_fuse_attr(&vattr, &req);
        Ok(ReplyCreated {
            ttl: Duration::from_secs(1),
            attr,
            generation: 0,
            fh: 0,
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
            .map_err(|e| -> Errno {
                let code = match e.as_str() {
                    "not found" => libc::ENOENT,
                    "parent not found" => libc::ENOENT,
                    "not a directory" => libc::ENOTDIR,
                    "already exists" => libc::EEXIST,
                    "is a directory" => libc::EISDIR,
                    "invalid name" => libc::EINVAL,
                    "invalid path" => libc::ENOENT,
                    "not supported" => libc::ENOSYS,
                    _ if e.contains("not supported") => libc::ENOSYS,
                    _ => libc::EIO,
                };
                code.into()
            })?;

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

        let (_ino, vattr) = self
            .create_symlink(&parent_path, target.as_ref())
            .await
            .map_err(|e| -> Errno {
                let code = match e.as_str() {
                    "invalid path" => libc::ENOENT,
                    "invalid name" => libc::EINVAL,
                    "parent not found" => libc::ENOENT,
                    "not a directory" => libc::ENOTDIR,
                    "already exists" => libc::EEXIST,
                    "not supported" => libc::ENOSYS,
                    _ if e.contains("not supported") => libc::ENOSYS,
                    _ => libc::EIO,
                };
                code.into()
            })?;

        let attr = vfs_to_fuse_attr(&vattr, &req);

        Ok(ReplyEntry {
            ttl: Duration::from_secs(1),
            attr,
            generation: 0,
        })
    }

    // Remove a file
    async fn unlink(&self, _req: Request, parent: u64, name: &OsStr) -> FuseResult<()> {
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
        if !matches!(cattr.kind, VfsFileType::File) {
            return Err(libc::EISDIR.into());
        }
        let Some(mut p) = self.path_of(parent as i64).await else {
            return Err(libc::ENOENT.into());
        };
        if p != "/" {
            p.push('/');
        }
        p.push_str(&name);
        self.unlink(&p).await.map_err(|e| {
            let code = match e.as_str() {
                "not found" => libc::ENOENT,
                "is a directory" => libc::EISDIR,
                _ => libc::EIO,
            };
            code.into()
        })
    }

    // Remove an empty directory
    async fn rmdir(&self, _req: Request, parent: u64, name: &OsStr) -> FuseResult<()> {
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
        self.rmdir(&p).await.map_err(|e| {
            let code = match e.as_str() {
                "not found" => libc::ENOENT,
                "directory not empty" => libc::ENOTEMPTY,
                _ => libc::EIO,
            };
            code.into()
        })
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
        let name = name.to_string_lossy();
        let new_name = new_name.to_string_lossy();
        // Ensure the source exists
        let Some(src_ino) = self.child_of(parent as i64, name.as_ref()).await else {
            return Err(libc::ENOENT.into());
        };
        let Some(_src_attr) = self.stat_ino(src_ino).await else {
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
        VFS::rename(self, &oldp, &newp).await.map_err(|e| match e {
            MetaError::AlreadyExists { .. } => libc::EEXIST.into(),
            MetaError::DirectoryNotEmpty(_) => libc::ENOTEMPTY.into(),
            MetaError::NotDirectory(_) => libc::ENOTDIR.into(),
            _ => libc::EIO.into(),
        })
    }

    // ===== Resource release & sync: stateless implementation, return success =====
    // Close file handle
    async fn release(
        &self,
        _req: Request,
        _inode: u64,
        _fh: u64,
        _flags: u32,
        _lock_owner: u64,
        _flush: bool,
    ) -> FuseResult<()> {
        Ok(())
    }

    // Flush file (close path callback)
    async fn flush(
        &self,
        _req: Request,
        _inode: u64,
        _fh: u64,
        _lock_owner: u64,
    ) -> FuseResult<()> {
        Ok(())
    }

    // Sync file content to backend
    async fn fsync(&self, _req: Request, _inode: u64, _fh: u64, _datasync: bool) -> FuseResult<()> {
        Ok(())
    }

    // Close directory handle
    async fn releasedir(&self, _req: Request, _inode: u64, fh: u64, _flags: u32) -> FuseResult<()> {
        if fh != 0 {
            self.closedir_handle(fh).await.ok();
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

    // Forget (kernel reference drop); no inode ref tracking yet so no-op
    async fn forget(&self, _req: Request, _inode: u64, _nlookup: u64) {}

    // Batch forget; no-op
    async fn batch_forget(&self, _req: Request, _inodes: &[(u64, u64)]) {}

    // Interrupt an in-flight request (no tracking), so no-op
    async fn interrupt(&self, _req: Request, _unique: u64) -> FuseResult<()> {
        Ok(())
    }
}

// =============== helpers ===============
fn vfs_kind_to_fuse(k: VfsFileType) -> FuseFileType {
    match k {
        VfsFileType::Dir => FuseFileType::Directory,
        VfsFileType::File => FuseFileType::RegularFile,
        VfsFileType::Symlink => FuseFileType::Symlink,
    }
}

fn vfs_to_fuse_attr(v: &VfsFileAttr, req: &Request) -> rfuse3::raw::reply::FileAttr {
    // Time/permission placeholders: derive mode from kind, timestamps use now
    let now = Timestamp::from(SystemTime::now());
    let perm = match v.kind {
        VfsFileType::Dir => 0o755,
        VfsFileType::File => 0o644,
        VfsFileType::Symlink => 0o777,
    } as u16;
    // blocks fields expressed in 512B units
    let blocks = v.size.div_ceil(512);
    rfuse3::raw::reply::FileAttr {
        ino: v.ino as u64,
        size: v.size,
        blocks,
        atime: now,
        mtime: now,
        ctime: now,
        #[cfg(target_os = "macos")]
        crtime: now,
        kind: vfs_kind_to_fuse(v.kind),
        perm,
        nlink: v.nlink,
        uid: req.uid,
        gid: req.gid,
        rdev: 0,
        #[cfg(target_os = "macos")]
        flags: 0,
        blksize: 4096,
    }
}
