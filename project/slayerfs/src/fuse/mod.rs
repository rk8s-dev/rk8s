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
use crate::vfs::fs::{FileAttr as VfsFileAttr, FileType as VfsFileType, VFS};
use bytes::Bytes;
use rfuse3::Result as FuseResult;
use rfuse3::raw::Request;
use rfuse3::raw::reply::{
    DirectoryEntry, DirectoryEntryPlus, ReplyAttr, ReplyCreated, ReplyData, ReplyDirectory,
    ReplyDirectoryPlus, ReplyEntry, ReplyInit, ReplyOpen, ReplyStatFs, ReplyWrite,
};
use std::ffi::{OsStr, OsString};
use std::num::NonZeroU32;
use std::pin::Pin;
use std::time::{Duration, SystemTime};

use futures_util::stream::{self, Stream};
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
    use crate::meta::create_meta_store_from_url;
    use std::fs;
    use std::io::Write;
    use std::time::Duration as StdDuration;

    // Linux 下的基本挂载冒烟测试：受环境变量 SLAYERFS_FUSE_TEST 控制
    #[tokio::test]
    async fn smoke_mount_and_basic_ops() {
        if std::env::var("SLAYERFS_FUSE_TEST").ok().as_deref() != Some("1") {
            eprintln!("skip fuse mount test: set SLAYERFS_FUSE_TEST=1 to enable");
            return;
        }

        let layout = ChunkLayout::default();
        let tmp_data = tempfile::tempdir().expect("tmp data");
        let client = ObjectClient::new(LocalFsBackend::new(tmp_data.path()));
        let store = ObjectBlockStore::new(client);

        let meta = create_meta_store_from_url("sqlite::memory:")
            .await
            .expect("create meta store");

        let fs = VFS::new(layout, store, meta).await.expect("create VFS");

        // 准备挂载点
        let mnt = tempfile::tempdir().expect("tmp mount");
        let mnt_path = mnt.path().to_path_buf();

        // 挂载（后台，直到卸载）
        let handle = match mount_vfs_unprivileged(fs, &mnt_path).await {
            Ok(h) => h,
            Err(e) => {
                eprintln!("skip fuse test: mount failed: {e}");
                return;
            }
        };

        // 给内核/守护线程一点时间完成 INIT
        tokio::time::sleep(StdDuration::from_millis(2000)).await;

        // 目录/文件基本操作
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

        // 列目录
        let list = fs::read_dir(&dir)
            .expect("readdir")
            .filter_map(|e| e.ok())
            .map(|e| e.file_name())
            .collect::<Vec<_>>();
        assert!(list.iter().any(|n| n.to_string_lossy() == "hello.txt"));

        // 删除并卸载
        fs::remove_file(&file_path).expect("unlink");

        // 主动卸载并等待结束
        if let Err(e) = handle.unmount().await {
            eprintln!("unmount error: {e}");
        }
    }
}
impl<S, M> Filesystem for VFS<S, M>
where
    S: BlockStore + Send + Sync + 'static,
    M: MetaStore + Send + Sync + 'static,
{
    // GAT：目录条目流（readdir）
    type DirEntryStream<'a>
        = Pin<Box<dyn Stream<Item = FuseResult<DirectoryEntry>> + Send + 'a>>
    where
        Self: 'a;

    // GAT：目录条目（plus）流（readdirplus）
    type DirEntryPlusStream<'a>
        = Pin<Box<dyn Stream<Item = FuseResult<DirectoryEntryPlus>> + Send + 'a>>
    where
        Self: 'a;

    async fn init(&self, _req: Request) -> FuseResult<ReplyInit> {
        // 选择一个保守的最大写入值（1MiB）。可按后端能力调整或做成可配置。
        let max_write = NonZeroU32::new(1024 * 1024).unwrap();
        Ok(ReplyInit { max_write })
    }

    async fn destroy(&self, _req: Request) {}

    // 调用 VFS：由 parent inode + name -> child inode；若找到，后续封装 ReplyEntry（暂占位）
    async fn lookup(&self, req: Request, parent: u64, name: &OsStr) -> FuseResult<ReplyEntry> {
        let name_str = name.to_string_lossy();
        let child = self.child_of(parent as i64, name_str.as_ref());
        let Some(child_ino) = child else {
            return Err(libc::ENOENT.into());
        };
        let Some(vattr) = self.stat_ino(child_ino).await else {
            return Err(libc::ENOENT.into());
        };
        let attr = vfs_to_fuse_attr(&vattr, &req);
        // generation 暂置 0；ttl 设为 1s，可调
        Ok(ReplyEntry {
            ttl: Duration::from_secs(1),
            attr,
            generation: 0,
        })
    }

    // 打开文件：此实现采用无状态 IO，返回 fh=0
    async fn open(&self, _req: Request, ino: u64, _flags: u32) -> FuseResult<ReplyOpen> {
        // 验证 inode 存在且是文件
        let Some(attr) = self.stat_ino(ino as i64).await else {
            return Err(libc::ENOENT.into());
        };
        if matches!(attr.kind, VfsFileType::Dir) {
            return Err(libc::EISDIR.into());
        }
        Ok(ReplyOpen { fh: 0, flags: 0 })
    }

    // 打开目录：无状态
    async fn opendir(&self, _req: Request, ino: u64, _flags: u32) -> FuseResult<ReplyOpen> {
        let Some(attr) = self.stat_ino(ino as i64).await else {
            return Err(libc::ENOENT.into());
        };
        if !matches!(attr.kind, VfsFileType::Dir) {
            return Err(libc::ENOTDIR.into());
        }
        Ok(ReplyOpen { fh: 0, flags: 0 })
    }

    // 读文件：映射到 VFS::read（通过 inode 构造路径）
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

    // 写文件：映射到 VFS::write（通过 inode 构造路径）
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
        let Some(path) = self.path_of(ino as i64) else {
            return Err(libc::ENOENT.into());
        };
        let n = self
            .write(&path, offset, data)
            .await
            .map_err(|_| libc::EIO)? as u32;
        Ok(ReplyWrite { written: n })
    }

    // 调用 VFS 获取 inode 属性（若 fh 有效则可忽略 flags）
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

    // 设置属性：当前仅支持 size（truncate）
    async fn setattr(
        &self,
        req: Request,
        ino: u64,
        _fh: Option<u64>,
        set_attr: SetAttr,
    ) -> FuseResult<ReplyAttr> {
        if let Some(size) = set_attr.size {
            let Some(path) = self.path_of(ino as i64) else {
                return Err(libc::ENOENT.into());
            };
            self.truncate(&path, size).await.map_err(|_| libc::EIO)?;
        }
        // 返回最新属性
        let Some(vattr) = self.stat_ino(ino as i64).await else {
            return Err(libc::ENOENT.into());
        };
        let attr = vfs_to_fuse_attr(&vattr, &req);
        Ok(ReplyAttr {
            ttl: Duration::from_secs(1),
            attr,
        })
    }

    // 调用 VFS 列目录，逐项构造 DirectoryEntry 并以流返回（含错误码检查与偏移处理）
    async fn readdir<'a>(
        &'a self,
        _req: Request,
        ino: u64,
        _fh: u64,
        offset: i64,
    ) -> FuseResult<ReplyDirectory<Self::DirEntryStream<'a>>> {
        let entries = match self.readdir_ino(ino as i64).await {
            None => {
                if self.stat_ino(ino as i64).await.is_some() {
                    return Err(libc::ENOTDIR.into());
                } else {
                    return Err(libc::ENOENT.into());
                }
            }
            Some(v) => v,
        };

        // 组装含 "." 与 ".." 的目录项，offset 为“上一个 entry 的偏移”，从 offset+1 开始输出
        let mut all: Vec<DirectoryEntry> = Vec::with_capacity(entries.len() + 2);
        // "."
        all.push(DirectoryEntry {
            inode: ino,
            kind: FuseFileType::Directory,
            name: OsString::from("."),
            offset: 1,
        });
        // ".."（父 inode 不易准确获取，先用 root 替代或自身）
        let parent_ino = self.parent_of(ino as i64).unwrap_or(self.root_ino()) as u64;
        all.push(DirectoryEntry {
            inode: parent_ino,
            kind: FuseFileType::Directory,
            name: OsString::from(".."),
            offset: 2,
        });
        // 真实子项
        for (i, e) in entries.iter().enumerate() {
            all.push(DirectoryEntry {
                inode: e.ino as u64,
                kind: vfs_kind_to_fuse(e.kind),
                name: OsString::from(e.name.clone()),
                offset: (i as i64) + 3,
            });
        }

        // 从 offset 后开始
        let start = if offset <= 0 { 0 } else { offset as usize };
        let slice = if start >= all.len() {
            Vec::new()
        } else {
            all[start..].to_vec()
        };
        let stream_iter = stream::iter(slice.into_iter().map(Ok));
        let boxed: Self::DirEntryStream<'a> = Box::pin(stream_iter);
        Ok(ReplyDirectory::<Self::DirEntryStream<'a>> { entries: boxed })
    }

    // 带属性的目录读取（lookup+readdir）：返回 DirectoryEntryPlus 流
    async fn readdirplus<'a>(
        &'a self,
        req: Request,
        ino: u64,
        _fh: u64,
        offset: u64,
        _lock_owner: u64,
    ) -> FuseResult<ReplyDirectoryPlus<Self::DirEntryPlusStream<'a>>> {
        let entries = match self.readdir_ino(ino as i64).await {
            None => {
                if self.stat_ino(ino as i64).await.is_some() {
                    return Err(libc::ENOTDIR.into());
                } else {
                    return Err(libc::ENOENT.into());
                }
            }
            Some(v) => v,
        };

        let ttl = Duration::from_secs(1);

        let mut all: Vec<DirectoryEntryPlus> = Vec::with_capacity(entries.len() + 2);
        // "."
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
        // ".."
        let parent_ino = self.parent_of(ino as i64).unwrap_or(self.root_ino()) as u64;
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
        // children
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
                offset: (i as i64) + 3,
                attr: fattr,
                entry_ttl: ttl,
                attr_ttl: ttl,
            });
        }

        // 按 offset 截断
        let start = if offset == 0 { 0 } else { offset as usize };
        let slice = if start >= all.len() {
            Vec::new()
        } else {
            all[start..].to_vec()
        };
        let stream_iter = stream::iter(slice.into_iter().map(Ok));
        let boxed: Self::DirEntryPlusStream<'a> = Box::pin(stream_iter);
        Ok(ReplyDirectoryPlus { entries: boxed })
    }

    // 文件系统统计：返回保守/占位值
    async fn statfs(&self, _req: Request, _ino: u64) -> FuseResult<ReplyStatFs> {
        // 由于此处无法安全读取内部实现细节，返回保守常量；后续可接入真实统计。
        let bsize: u32 = 4096;
        let frsize: u32 = 4096;
        let files: u64 = 0;
        let ffree: u64 = u64::MAX;
        // blocks/bfree/bavail 暂未知，返回 0；namelen 取常见上限 255。
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

    // 创建目录（单级）。若已存在则返回 EEXIST。
    async fn mkdir(
        &self,
        req: Request,
        parent: u64,
        name: &OsStr,
        _mode: u32,
        _umask: u32,
    ) -> FuseResult<ReplyEntry> {
        let name = name.to_string_lossy();
        // 父必须是目录
        let Some(pattr) = self.stat_ino(parent as i64).await else {
            return Err(libc::ENOENT.into());
        };
        if !matches!(pattr.kind, VfsFileType::Dir) {
            return Err(libc::ENOTDIR.into());
        }
        // 冲突检查
        if let Some(_child) = self.child_of(parent as i64, name.as_ref()) {
            return Err(libc::EEXIST.into());
        }
        // 构造路径并创建
        let Some(mut p) = self.path_of(parent as i64) else {
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

    // 创建并打开文件
    async fn create(
        &self,
        req: Request,
        parent: u64,
        name: &OsStr,
        _mode: u32,
        _flags: u32,
    ) -> FuseResult<ReplyCreated> {
        let name = name.to_string_lossy();
        // 父检查
        let Some(pattr) = self.stat_ino(parent as i64).await else {
            return Err(libc::ENOENT.into());
        };
        if !matches!(pattr.kind, VfsFileType::Dir) {
            return Err(libc::ENOTDIR.into());
        }
        let Some(mut p) = self.path_of(parent as i64) else {
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

    // 删除文件
    async fn unlink(&self, _req: Request, parent: u64, name: &OsStr) -> FuseResult<()> {
        let name = name.to_string_lossy();
        // 父目录存在与类型检查
        let Some(pattr) = self.stat_ino(parent as i64).await else {
            return Err(libc::ENOENT.into());
        };
        if !matches!(pattr.kind, VfsFileType::Dir) {
            return Err(libc::ENOTDIR.into());
        }
        // 目标必须存在且为文件
        let Some(child) = self.child_of(parent as i64, name.as_ref()) else {
            return Err(libc::ENOENT.into());
        };
        let Some(cattr) = self.stat_ino(child).await else {
            return Err(libc::ENOENT.into());
        };
        if !matches!(cattr.kind, VfsFileType::File) {
            return Err(libc::EISDIR.into());
        }
        let Some(mut p) = self.path_of(parent as i64) else {
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

    // 删除空目录
    async fn rmdir(&self, _req: Request, parent: u64, name: &OsStr) -> FuseResult<()> {
        let name = name.to_string_lossy();
        let Some(pattr) = self.stat_ino(parent as i64).await else {
            return Err(libc::ENOENT.into());
        };
        if !matches!(pattr.kind, VfsFileType::Dir) {
            return Err(libc::ENOTDIR.into());
        }
        // 目标应为目录
        let Some(child) = self.child_of(parent as i64, name.as_ref()) else {
            return Err(libc::ENOENT.into());
        };
        let Some(cattr) = self.stat_ino(child).await else {
            return Err(libc::ENOENT.into());
        };
        if !matches!(cattr.kind, VfsFileType::Dir) {
            return Err(libc::ENOTDIR.into());
        }
        let Some(mut p) = self.path_of(parent as i64) else {
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

    // 重命名（支持文件和目录）
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
        // 检查源存在
        let Some(src_ino) = self.child_of(parent as i64, name.as_ref()) else {
            return Err(libc::ENOENT.into());
        };
        let Some(_src_attr) = self.stat_ino(src_ino).await else {
            return Err(libc::ENOENT.into());
        };

        // 检查目标父
        let Some(pattr) = self.stat_ino(new_parent as i64).await else {
            return Err(libc::ENOENT.into());
        };
        if !matches!(pattr.kind, VfsFileType::Dir) {
            return Err(libc::ENOTDIR.into());
        }

        // 目标存在性检查
        if self
            .child_of(new_parent as i64, new_name.as_ref())
            .is_some()
        {
            return Err(libc::EEXIST.into());
        }

        // 拼接路径并执行
        let Some(mut oldp) = self.path_of(parent as i64) else {
            return Err(libc::ENOENT.into());
        };
        if oldp != "/" {
            oldp.push('/');
        }
        oldp.push_str(&name);
        let Some(mut newp) = self.path_of(new_parent as i64) else {
            return Err(libc::ENOENT.into());
        };
        if newp != "/" {
            newp.push('/');
        }
        newp.push_str(&new_name);
        VFS::rename(self, &oldp, &newp).await.map_err(|e| {
            let code = match e.as_str() {
                "target exists" => libc::EEXIST,
                _ => libc::EIO,
            };
            code.into()
        })
    }

    // ===== 资源释放与同步：无状态实现，直接成功返回 =====
    // 关闭文件句柄
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

    // 刷新文件（close 路径的回调）
    async fn flush(
        &self,
        _req: Request,
        _inode: u64,
        _fh: u64,
        _lock_owner: u64,
    ) -> FuseResult<()> {
        Ok(())
    }

    // 同步文件内容到后端
    async fn fsync(&self, _req: Request, _inode: u64, _fh: u64, _datasync: bool) -> FuseResult<()> {
        Ok(())
    }

    // 关闭目录句柄
    async fn releasedir(
        &self,
        _req: Request,
        _inode: u64,
        _fh: u64,
        _flags: u32,
    ) -> FuseResult<()> {
        Ok(())
    }

    // 同步目录内容到后端
    async fn fsyncdir(
        &self,
        _req: Request,
        _inode: u64,
        _fh: u64,
        _datasync: bool,
    ) -> FuseResult<()> {
        Ok(())
    }

    // 忘记（内核引用计数回收），当前不维护 inode 引用，no-op
    async fn forget(&self, _req: Request, _inode: u64, _nlookup: u64) {}

    // 批量忘记（batch forget），no-op
    async fn batch_forget(&self, _req: Request, _inodes: &[(u64, u64)]) {}

    // 中断某个进行中的请求（目前无内部跟踪），no-op
    async fn interrupt(&self, _req: Request, _unique: u64) -> FuseResult<()> {
        Ok(())
    }
}

// =============== helpers ===============
fn vfs_kind_to_fuse(k: VfsFileType) -> FuseFileType {
    match k {
        VfsFileType::Dir => FuseFileType::Directory,
        VfsFileType::File => FuseFileType::RegularFile,
    }
}

fn vfs_to_fuse_attr(v: &VfsFileAttr, req: &Request) -> rfuse3::raw::reply::FileAttr {
    // 时间与权限占位：按 kind 赋默认权限；时间用当前时间
    let now = Timestamp::from(SystemTime::now());
    let perm = match v.kind {
        VfsFileType::Dir => 0o755,
        VfsFileType::File => 0o644,
    } as u16;
    // blocks 字段按 512B 块计算
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
        nlink: 1,
        uid: req.uid,
        gid: req.gid,
        rdev: 0,
        #[cfg(target_os = "macos")]
        flags: 0,
        blksize: 4096,
    }
}
