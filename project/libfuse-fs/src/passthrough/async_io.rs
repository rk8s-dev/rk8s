use crate::util::open_options::OpenOptions;
use bytes::Bytes;
use futures::stream;
use libc::{off_t, pread, size_t};
use rfuse3::{Errno, Inode, Result, raw::prelude::*};
use std::{
    ffi::{CStr, CString, OsStr, OsString},
    fs::File,
    io,
    mem::MaybeUninit,
    num::NonZeroU32,
    os::{
        fd::{AsRawFd, RawFd},
        raw::c_int,
        unix::ffi::OsStringExt,
    },
    sync::{Arc, atomic::Ordering},
    time::Duration,
};
use tracing::{debug, error, info, trace};

use vm_memory::{ByteValued, bitmap::BitmapSlice};

use crate::{
    passthrough::{CURRENT_DIR_CSTR, EMPTY_CSTR, FileUniqueKey, PARENT_DIR_CSTR, statx::statx},
    util::{convert_stat64_to_file_attr, filetype_from_mode},
};

use super::{
    Handle, HandleData, PassthroughFs, config::CachePolicy, os_compat::LinuxDirent64, util::*,
};

impl<S: BitmapSlice + Send + Sync> PassthroughFs<S> {
    async fn open_inode(&self, inode: Inode, flags: i32) -> io::Result<File> {
        let data = self.inode_map.get(inode).await?;
        if !is_safe_inode(data.mode) {
            Err(ebadf())
        } else {
            let mut new_flags = self.get_writeback_open_flags(flags).await;
            if !self.cfg.allow_direct_io && flags & libc::O_DIRECT != 0 {
                new_flags &= !libc::O_DIRECT;
            }
            data.open_file(new_flags | libc::O_CLOEXEC, &self.proc_self_fd)
        }
    }

    /// Check the HandleData flags against the flags from the current request
    /// if these do not match update the file descriptor flags and store the new
    /// result in the HandleData entry
    async fn check_fd_flags(
        &self,
        data: &Arc<HandleData>,
        fd: RawFd,
        flags: u32,
    ) -> io::Result<()> {
        let open_flags = data.get_flags().await;
        if open_flags != flags {
            let ret = unsafe { libc::fcntl(fd, libc::F_SETFL, flags) };
            if ret != 0 {
                return Err(io::Error::last_os_error());
            }
            data.set_flags(flags).await;
        }
        Ok(())
    }

    async fn do_readdir(
        &self,
        inode: Inode,
        handle: Handle,
        offset: u64,
        entry_list: &mut Vec<std::result::Result<DirectoryEntry, Errno>>,
    ) -> io::Result<()> {
        const BUFFER_SIZE: usize = 8192;

        let data = self.get_dirdata(handle, inode, libc::O_RDONLY).await?;

        // Since we are going to work with the kernel offset, we have to acquire the file lock
        // for both the `lseek64` and `getdents64` syscalls to ensure that no other thread
        // changes the kernel offset while we are using it.
        let (_guard, dir) = data.get_file_mut().await;

        // Allocate buffer; pay attention to alignment.
        let mut buffer = vec![0u8; BUFFER_SIZE];

        // Safe because this doesn't modify any memory and we check the return value.
        let res =
            unsafe { libc::lseek64(dir.as_raw_fd(), offset as libc::off64_t, libc::SEEK_SET) };
        if res < 0 {
            return Err(io::Error::last_os_error());
        }

        loop {
            // call getdents64 system call
            let result = unsafe {
                libc::syscall(
                    libc::SYS_getdents64,
                    dir.as_raw_fd(),
                    buffer.as_mut_ptr() as *mut LinuxDirent64,
                    BUFFER_SIZE,
                )
            };

            if result == -1 {
                return Err(std::io::Error::last_os_error());
            }

            let bytes_read = result as usize;
            if bytes_read == 0 {
                break; // no more
            }

            // push every entry .
            let mut offset = 0;
            while offset < bytes_read {
                //let (front, back) = buffer.split_at(size_of::<LinuxDirent64>());
                //size_of::<LinuxDirent64>()
                let front = &buffer[offset..offset + size_of::<LinuxDirent64>()];
                let back = &buffer[offset + size_of::<LinuxDirent64>()..];

                let dirent64 = LinuxDirent64::from_slice(front)
                    .expect("fuse: unable to get LinuxDirent64 from slice");

                let namelen = dirent64.d_reclen as usize - size_of::<LinuxDirent64>();
                debug_assert!(
                    namelen <= back.len(),
                    "fuse: back is smaller than `namelen`"
                );

                let name = &back[..namelen];
                if name.eq(CURRENT_DIR_CSTR) || name.eq(PARENT_DIR_CSTR) {
                    offset += dirent64.d_reclen as usize;
                    continue;
                }
                let name = bytes_to_cstr(name)
                    .map_err(|e| {
                        error!("fuse: do_readdir: {e:?}");
                        einval()
                    })?
                    .to_bytes();

                let mut entry = DirectoryEntry {
                    inode: dirent64.d_ino,
                    kind: filetype_from_mode((dirent64.d_ty as u16 * 0x1000u16).into()),
                    name: OsString::from_vec(name.to_vec()),
                    offset: dirent64.d_off,
                };
                // Safe because do_readdir() has ensured dir_entry.name is a
                // valid [u8] generated by CStr::to_bytes().
                let name = osstr_to_cstr(&entry.name)?;
                // trace!("do_readdir: inode={}, name={}", inode, name.to_str().unwrap());
                let _entry = self.do_lookup(inode, &name).await?;
                let mut inodes = self.inode_map.inodes.write().await;

                self.forget_one(&mut inodes, _entry.attr.ino, 1).await;
                entry.inode = _entry.attr.ino;
                entry_list.push(Ok(entry));

                // move to next entry
                offset += dirent64.d_reclen as usize;
            }
        }

        Ok(())
    }

    async fn do_readdirplus(
        &self,
        inode: Inode,
        handle: Handle,
        offset: u64,
        entry_list: &mut Vec<std::result::Result<DirectoryEntryPlus, Errno>>,
    ) -> io::Result<()> {
        const BUFFER_SIZE: usize = 8192;

        let data = self.get_dirdata(handle, inode, libc::O_RDONLY).await?;

        // Since we are going to work with the kernel offset, we have to acquire the file lock
        // for both the `lseek64` and `getdents64` syscalls to ensure that no other thread
        // changes the kernel offset while we are using it.
        let (_guard, dir) = data.get_file_mut().await;

        // Allocate buffer; pay attention to alignment.
        let mut buffer = vec![0u8; BUFFER_SIZE];

        // Safe because this doesn't modify any memory and we check the return value.
        let res =
            unsafe { libc::lseek64(dir.as_raw_fd(), offset as libc::off64_t, libc::SEEK_SET) };
        if res < 0 {
            return Err(io::Error::last_os_error());
        }
        loop {
            // call getdents64 system call
            let result = unsafe {
                libc::syscall(
                    libc::SYS_getdents64,
                    dir.as_raw_fd(),
                    buffer.as_mut_ptr() as *mut LinuxDirent64,
                    BUFFER_SIZE,
                )
            };

            if result == -1 {
                return Err(std::io::Error::last_os_error());
            }

            let bytes_read = result as usize;
            if bytes_read == 0 {
                break;
            }

            let mut offset = 0;
            while offset < bytes_read {
                //size_of::<LinuxDirent64>()
                let front = &buffer[offset..offset + size_of::<LinuxDirent64>()];
                let back = &buffer[offset + size_of::<LinuxDirent64>()..];
                //let (front, back) = buffer.split_at(size_of::<LinuxDirent64>());

                let dirent64 = LinuxDirent64::from_slice(front)
                    .expect("fuse: unable to get LinuxDirent64 from slice");

                let namelen = dirent64.d_reclen as usize - size_of::<LinuxDirent64>();
                debug_assert!(
                    namelen <= back.len(),
                    "fuse: back is smaller than `namelen`"
                );

                let name = &back[..namelen];
                if name.starts_with(CURRENT_DIR_CSTR) || name.starts_with(PARENT_DIR_CSTR) {
                    offset += dirent64.d_reclen as usize;
                    continue;
                }
                let name = bytes_to_cstr(name)
                    .map_err(|e| {
                        error!("fuse: do_readdir: {e:?}");
                        einval()
                    })?
                    .to_bytes();

                let mut entry = DirectoryEntry {
                    inode: dirent64.d_ino,
                    kind: filetype_from_mode((dirent64.d_ty as u16 * 0x1000u16).into()),
                    name: OsString::from_vec(name.to_vec()),
                    offset: dirent64.d_off,
                };
                // Safe because do_readdir() has ensured dir_entry.name is a
                // valid [u8] generated by CStr::to_bytes().
                let name = osstr_to_cstr(&entry.name)?;
                debug!("readdir:{}", name.to_str().unwrap());
                let _entry = self.do_lookup(inode, &name).await?;
                entry.inode = _entry.attr.ino;

                entry_list.push(Ok(DirectoryEntryPlus {
                    inode: entry.inode,
                    generation: _entry.generation,
                    kind: entry.kind,
                    name: entry.name,
                    offset: entry.offset,
                    attr: _entry.attr,
                    entry_ttl: _entry.ttl,
                    attr_ttl: _entry.ttl,
                }));
                // add the offset.
                offset += dirent64.d_reclen as usize;
            }
        }
        Ok(())
    }

    async fn do_open(&self, inode: Inode, flags: u32) -> io::Result<(Option<Handle>, OpenOptions)> {
        let file = self.open_inode(inode, flags as i32).await?;

        let data = HandleData::new(inode, file, flags);
        let handle = self.next_handle.fetch_add(1, Ordering::Relaxed);
        self.handle_map.insert(handle, data).await;

        let mut opts = OpenOptions::empty();
        match self.cfg.cache_policy {
            // We only set the direct I/O option on files.
            CachePolicy::Never => opts.set(
                OpenOptions::DIRECT_IO,
                flags & (libc::O_DIRECTORY as u32) == 0,
            ),
            CachePolicy::Metadata => {
                if flags & (libc::O_DIRECTORY as u32) == 0 {
                    opts |= OpenOptions::DIRECT_IO;
                } else {
                    opts |= OpenOptions::CACHE_DIR | OpenOptions::KEEP_CACHE;
                }
            }
            CachePolicy::Always => {
                opts |= OpenOptions::KEEP_CACHE;
                if flags & (libc::O_DIRECTORY as u32) != 0 {
                    opts |= OpenOptions::CACHE_DIR;
                }
            }
            _ => {}
        };

        Ok((Some(handle), opts))
    }

    /// Core implementation for `getattr`.
    ///
    /// This is the internal function that performs the actual `stat` system call.
    /// It contains a crucial `mapping` parameter that controls its behavior:
    /// - `mapping: true`: Applies reverse ID mapping (host -> container) to the `uid` and `gid`.
    ///   This is for external FUSE clients.
    /// - `mapping: false`: Returns the raw, unmapped host attributes. This is for internal
    ///   callers like `overlayfs`'s copy-up logic.
    pub(crate) async fn do_getattr_inner(
        &self,
        inode: Inode,
        handle: Option<Handle>,
        mapping: bool,
    ) -> io::Result<(libc::stat64, Duration)> {
        // trace!("FS {} passthrough: do_getattr: before get: inode={}, handle={:?}", self.uuid, inode, handle);
        let data = self.inode_map.get(inode).await.map_err(|e| {
            error!("fuse: do_getattr ino {inode} Not find err {e:?}");
            e
        })?;
        // trace!("do_getattr: got data {:?}", data);

        // kernel sends 0 as handle in case of no_open, and it depends on fuse server to handle
        // this case correctly.
        let st = if !self.no_open.load(Ordering::Relaxed)
            && let Some(handle_id) = handle
        {
            let hd = self.handle_map.get(handle_id, inode).await?;
            // trace!("FS {} passthrough: do_getattr: before stat_fd", self.uuid);
            stat_fd(hd.get_file(), None)
        } else {
            // trace!("FS {} passthrough: do_getattr: before stat", self.uuid);
            data.handle.stat()
        };
        // trace!("FS {} passthrough: do_getattr: after stat", self.uuid);

        let mut st = st.map_err(|e| {
            error!("fuse: do_getattr stat failed ino {inode} err {e:?}");
            e
        })?;
        st.st_ino = inode;
        if mapping {
            st.st_uid = self.cfg.mapping.find_mapping(st.st_uid, true, true);
            st.st_gid = self.cfg.mapping.find_mapping(st.st_gid, true, false);
        }
        Ok((st, self.cfg.attr_timeout))
    }

    /// Public `getattr` wrapper for FUSE clients.
    ///
    /// This function serves as the standard entry point for `getattr` requests from the FUSE
    /// kernel module. It always performs ID mapping by calling [`do_getattr_inner`][Self::do_getattr_inner] with
    /// `mapping: true` to ensure clients see attributes from the container's perspective.
    async fn do_getattr(
        &self,
        inode: Inode,
        handle: Option<Handle>,
    ) -> io::Result<(libc::stat64, Duration)> {
        self.do_getattr_inner(inode, handle, true).await
    }

    /// Internal `getattr` helper that skips ID mapping.
    ///
    /// This helper is specifically designed for internal use by `overlayfs`. It calls
    /// [`do_getattr_inner`][Self::do_getattr_inner] with `mapping: false` to retrieve the raw, unmodified host
    /// attributes of a file. This is essential for the `copy_up` process to correctly
    /// preserve the original file ownership.
    pub(crate) async fn do_getattr_helper(
        &self,
        inode: Inode,
        handle: Option<Handle>,
    ) -> io::Result<(libc::stat64, Duration)> {
        self.do_getattr_inner(inode, handle, false).await
    }

    async fn do_unlink(&self, parent: Inode, name: &CStr, flags: libc::c_int) -> io::Result<()> {
        let data = self.inode_map.get(parent).await?;
        let file = data.get_file()?;
        let st = statx(&file, Some(name)).ok();
        // Safe because this doesn't modify any memory and we check the return value.
        let res = unsafe { libc::unlinkat(file.as_raw_fd(), name.as_ptr(), flags) };
        if res == 0 {
            if let Some(st) = st
                && let Some(btime) = st.btime
                && (btime.tv_sec != 0 || btime.tv_nsec != 0)
            {
                let key = FileUniqueKey(st.st.st_ino, btime);
                self.handle_cache.invalidate(&key).await;
            }

            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }

    async fn get_dirdata(
        &self,
        handle: Handle,
        inode: Inode,
        flags: libc::c_int,
    ) -> io::Result<Arc<HandleData>> {
        let no_open = self.no_opendir.load(Ordering::Relaxed);
        if !no_open {
            self.handle_map.get(handle, inode).await
        } else {
            let file = self.open_inode(inode, flags | libc::O_DIRECTORY).await?;
            Ok(Arc::new(HandleData::new(inode, file, flags as u32)))
        }
    }

    async fn get_data(
        &self,
        handle: Handle,
        inode: Inode,
        flags: libc::c_int,
    ) -> io::Result<Arc<HandleData>> {
        let no_open = self.no_open.load(Ordering::Relaxed);
        if !no_open {
            self.handle_map.get(handle, inode).await
        } else {
            let file = self.open_inode(inode, flags).await?;
            Ok(Arc::new(HandleData::new(inode, file, flags as u32)))
        }
    }

    /// Core implementation for `create`.
    ///
    /// It uses the provided `uid` and `gid` for credential switching if they are `Some`;
    /// otherwise, it falls back to the credentials from the `Request`. This allows internal
    /// callers like `overlayfs` to specify an exact host UID/GID.
    #[allow(clippy::too_many_arguments)]
    async fn do_create_inner(
        &self,
        req: Request,
        parent: Inode,
        name: &OsStr,
        mode: u32,
        flags: u32,
        uid: Option<u32>,
        gid: Option<u32>,
    ) -> Result<ReplyCreated> {
        let name = osstr_to_cstr(name).unwrap();
        let name = name.as_ref();
        self.validate_path_component(name)?;

        let dir = self.inode_map.get(parent).await?;
        let dir_file = dir.get_file()?;

        let new_file = {
            // Here we need to adjust the code order because guard doesn't allowed to cross await point
            let flags = self.get_writeback_open_flags(flags as i32).await;
            let _guard = set_creds(
                uid.unwrap_or(self.cfg.mapping.get_uid(req.uid)),
                gid.unwrap_or(self.cfg.mapping.get_gid(req.gid)),
            )?;
            Self::create_file_excl(&dir_file, name, flags, mode)?
        };

        let entry = self.do_lookup(parent, name).await?;
        let file = match new_file {
            // File didn't exist, now created by create_file_excl()
            Some(f) => f,
            // File exists, and args.flags doesn't contain O_EXCL. Now let's open it with
            // open_inode().
            None => {
                // Cap restored when _killpriv is dropped
                // let _killpriv = if self.killpriv_v2.load().await
                //     && (args.fuse_flags & FOPEN_IN_KILL_SUIDGID != 0)
                // {
                //     self::drop_cap_fsetid()?
                // } else {
                //     None
                // };

                // Here we can not call self.open_inode() directly because guard doesn't allowed to cross await point
                let data = self.inode_map.get(entry.attr.ino).await?;
                if !is_safe_inode(data.mode) {
                    return Err(ebadf().into());
                }

                // Calculate the final flags. This involves an async call.
                let mut final_flags = self.get_writeback_open_flags(flags as i32).await;
                if !self.cfg.allow_direct_io && (flags as i32) & libc::O_DIRECT != 0 {
                    final_flags &= !libc::O_DIRECT;
                }
                final_flags |= libc::O_CLOEXEC;

                {
                    let _guard = set_creds(
                        uid.unwrap_or(self.cfg.mapping.get_uid(req.uid)),
                        gid.unwrap_or(self.cfg.mapping.get_gid(req.gid)),
                    )?;
                    // Maybe buggy because `open_file` may call `open_by_handle_at`, which requires CAP_DAC_READ_SEARCH.
                    data.open_file(final_flags, &self.proc_self_fd)?
                }
            }
        };

        let ret_handle = if !self.no_open.load(Ordering::Relaxed) {
            let handle = self.next_handle.fetch_add(1, Ordering::Relaxed);
            let data = HandleData::new(entry.attr.ino, file, flags);
            self.handle_map.insert(handle, data).await;
            handle
        } else {
            return Err(io::Error::from_raw_os_error(libc::EACCES).into());
        };

        let mut opts = OpenOptions::empty();
        match self.cfg.cache_policy {
            CachePolicy::Never => opts |= OpenOptions::DIRECT_IO,
            CachePolicy::Metadata => opts |= OpenOptions::DIRECT_IO,
            CachePolicy::Always => opts |= OpenOptions::KEEP_CACHE,
            _ => {}
        };
        Ok(ReplyCreated {
            ttl: entry.ttl,
            attr: entry.attr,
            generation: entry.generation,
            fh: ret_handle,
            flags: opts.bits(),
        })
    }

    /// A wrapper for `create`, used by [`copy_regfile_up`][crate::overlayfs::OverlayFs::copy_regfile_up].
    ///
    /// This helper is called during a copy-up operation to create a file in the upper
    /// layer while preserving the original host UID/GID from the lower layer file.
    #[allow(clippy::too_many_arguments)]
    pub async fn do_create_helper(
        &self,
        req: Request,
        parent: Inode,
        name: &OsStr,
        mode: u32,
        flags: u32,
        uid: u32,
        gid: u32,
    ) -> Result<ReplyCreated> {
        self.do_create_inner(req, parent, name, mode, flags, Some(uid), Some(gid))
            .await
    }

    /// Core implementation for `mkdir`.
    ///
    /// It uses the provided `uid` and `gid` for credential switching if they are `Some`;
    /// otherwise, it falls back to the credentials from the `Request`.
    #[allow(clippy::too_many_arguments)]
    async fn do_mkdir_inner(
        &self,
        req: Request,
        parent: Inode,
        name: &OsStr,
        mode: u32,
        umask: u32,
        uid: Option<u32>,
        gid: Option<u32>,
    ) -> Result<ReplyEntry> {
        let name = osstr_to_cstr(name).unwrap();
        let name = name.as_ref();
        self.validate_path_component(name)?;

        let data = self.inode_map.get(parent).await?;
        let file = data.get_file()?;

        let res = {
            let _guard = set_creds(
                uid.unwrap_or(self.cfg.mapping.get_uid(req.uid)),
                gid.unwrap_or(self.cfg.mapping.get_gid(req.gid)),
            )?;

            // Safe because this doesn't modify any memory and we check the return value.
            unsafe { libc::mkdirat(file.as_raw_fd(), name.as_ptr(), mode & !umask) }
        };
        if res < 0 {
            return Err(io::Error::last_os_error().into());
        }

        self.do_lookup(parent, name).await
    }

    /// A wrapper for `mkdir`, used by [`create_upper_dir`][crate::overlayfs::OverlayInode::create_upper_dir] function.
    ///
    /// This helper is called during a copy-up operation when a parent directory needs to be
    /// created in the upper layer, preserving the original host UID/GID.
    #[allow(clippy::too_many_arguments)]
    pub async fn do_mkdir_helper(
        &self,
        req: Request,
        parent: Inode,
        name: &OsStr,
        mode: u32,
        umask: u32,
        uid: u32,
        gid: u32,
    ) -> Result<ReplyEntry> {
        self.do_mkdir_inner(req, parent, name, mode, umask, Some(uid), Some(gid))
            .await
    }

    /// Core implementation for `symlink`.
    ///
    /// It uses the provided `uid` and `gid` for credential switching if they are `Some`;
    /// otherwise, it falls back to the credentials from the `Request`.
    async fn do_symlink_inner(
        &self,
        req: Request,
        parent: Inode,
        name: &OsStr,
        link: &OsStr,
        uid: Option<u32>,
        gid: Option<u32>,
    ) -> Result<ReplyEntry> {
        let name = osstr_to_cstr(name).unwrap();
        let name = name.as_ref();
        let link = osstr_to_cstr(link).unwrap();
        let link = link.as_ref();
        self.validate_path_component(name)?;

        let data = self.inode_map.get(parent).await?;
        let file = data.get_file()?;

        let res = {
            let _guard = set_creds(
                uid.unwrap_or(self.cfg.mapping.get_uid(req.uid)),
                gid.unwrap_or(self.cfg.mapping.get_gid(req.gid)),
            )?;

            // Safe because this doesn't modify any memory and we check the return value.
            unsafe { libc::symlinkat(link.as_ptr(), file.as_raw_fd(), name.as_ptr()) }
        };
        if res == 0 {
            self.do_lookup(parent, name).await
        } else {
            Err(io::Error::last_os_error().into())
        }
    }

    /// A wrapper for `symlink`, used by [`copy_symlink_up`][crate::overlayfs::OverlayFs::copy_symlink_up] function.
    ///
    /// This helper is called during a copy-up operation to create a symbolic link in the
    /// upper layer while preserving the original host UID/GID from the lower layer link.
    pub async fn do_symlink_helper(
        &self,
        req: Request,
        parent: Inode,
        name: &OsStr,
        link: &OsStr,
        uid: u32,
        gid: u32,
    ) -> Result<ReplyEntry> {
        self.do_symlink_inner(req, parent, name, link, Some(uid), Some(gid))
            .await
    }
}

impl Filesystem for PassthroughFs {
    /// initialize filesystem. Called before any other filesystem method.
    async fn init(&self, _req: Request) -> Result<ReplyInit> {
        if self.cfg.do_import {
            self.import().await?;
        }

        Ok(ReplyInit {
            max_write: NonZeroU32::new(128 * 1024).unwrap(),
        })
    }

    /// clean up filesystem. Called on filesystem exit which is fuseblk, in normal fuse filesystem,
    /// kernel may call forget for root. There is some discuss for this
    /// <https://github.com/bazil/fuse/issues/82#issuecomment-88126886>,
    /// <https://sourceforge.net/p/fuse/mailman/message/31995737/>
    async fn destroy(&self, _req: Request) {
        self.handle_map.clear().await;
        self.inode_map.clear().await;

        if let Err(e) = self.import().await {
            error!("fuse: failed to destroy instance, {e:?}");
        };
    }

    /// look up a directory entry by name and get its attributes.
    async fn lookup(&self, _req: Request, parent: Inode, name: &OsStr) -> Result<ReplyEntry> {
        // Don't use is_safe_path_component(), allow "." and ".." for NFS export support
        if name.to_string_lossy().as_bytes().contains(&SLASH_ASCII) {
            return Err(einval().into());
        }
        let name = osstr_to_cstr(name).unwrap();
        // trace!("lookup: parent={}, name={}", parent, name.to_str().unwrap());
        self.do_lookup(parent, name.as_ref()).await
    }

    /// forget an inode. The nlookup parameter indicates the number of lookups previously
    /// performed on this inode. If the filesystem implements inode lifetimes, it is recommended
    /// that inodes acquire a single reference on each lookup, and lose nlookup references on each
    /// forget. The filesystem may ignore forget calls, if the inodes don't need to have a limited
    /// lifetime. On unmount it is not guaranteed, that all referenced inodes will receive a forget
    /// message. When filesystem is normal(not fuseblk) and unmounting, kernel may send forget
    /// request for root and this library will stop session after call forget. There is some
    /// discussion for this <https://github.com/bazil/fuse/issues/82#issuecomment-88126886>,
    /// <https://sourceforge.net/p/fuse/mailman/message/31995737/>
    async fn forget(&self, _req: Request, inode: Inode, nlookup: u64) {
        let mut inodes = self.inode_map.inodes.write().await;

        self.forget_one(&mut inodes, inode, nlookup).await
    }

    /// get file attributes. If `fh` is None, means `fh` is not set.
    async fn getattr(
        &self,
        _req: Request,
        inode: Inode,
        fh: Option<u64>,
        _flags: u32,
    ) -> Result<ReplyAttr> {
        let re = self.do_getattr(inode, fh).await?;
        Ok(ReplyAttr {
            ttl: re.1,
            attr: convert_stat64_to_file_attr(re.0),
        })
    }

    /// set file attributes. If `fh` is None, means `fh` is not set.
    async fn setattr(
        &self,
        req: Request,
        inode: Inode,
        fh: Option<u64>,
        set_attr: SetAttr,
    ) -> Result<ReplyAttr> {
        let inode_data = self.inode_map.get(inode).await?;

        enum Data {
            Handle(Arc<HandleData>),
            ProcPath(CString),
        }

        let file = inode_data.get_file()?;
        let data = if self.no_open.load(Ordering::Relaxed) {
            let pathname = CString::new(format!("{}", file.as_raw_fd()))
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
            Data::ProcPath(pathname)
        } else {
            // If we have a handle then use it otherwise get a new fd from the inode.
            if let Some(handle) = fh {
                let hd = self.handle_map.get(handle, inode).await?;
                Data::Handle(hd)
            } else {
                let pathname = CString::new(format!("{}", file.as_raw_fd()))
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
                Data::ProcPath(pathname)
            }
        };

        if set_attr.size.is_some() && self.seal_size.load(Ordering::Relaxed) {
            return Err(io::Error::from_raw_os_error(libc::EPERM).into());
        }

        if set_attr.mode.is_some() {
            // Safe because this doesn't modify any memory and we check the return value.
            let res = unsafe {
                match data {
                    Data::Handle(ref h) => {
                        libc::fchmod(h.borrow_fd().as_raw_fd(), set_attr.mode.unwrap())
                    }
                    Data::ProcPath(ref p) => libc::fchmodat(
                        self.proc_self_fd.as_raw_fd(),
                        p.as_ptr(),
                        set_attr.mode.unwrap(),
                        0,
                    ),
                }
            };
            if res < 0 {
                return Err(io::Error::last_os_error().into());
            }
        }

        if set_attr.uid.is_some() && set_attr.gid.is_some() {
            //valid.intersects(SetattrValid::UID | SetattrValid::GID)
            let uid = self.cfg.mapping.get_uid(set_attr.uid.unwrap());
            let gid = self.cfg.mapping.get_gid(set_attr.gid.unwrap());

            // Safe because this is a constant value and a valid C string.
            let empty = unsafe { CStr::from_bytes_with_nul_unchecked(EMPTY_CSTR) };

            // Safe because this doesn't modify any memory and we check the return value.
            let res = unsafe {
                libc::fchownat(
                    file.as_raw_fd(),
                    empty.as_ptr(),
                    uid,
                    gid,
                    libc::AT_EMPTY_PATH | libc::AT_SYMLINK_NOFOLLOW,
                )
            };
            if res < 0 {
                return Err(io::Error::last_os_error().into());
            }
        }

        if set_attr.size.is_some() {
            let size = set_attr.size.unwrap();
            // Safe because this doesn't modify any memory and we check the return value.
            let res = match data {
                Data::Handle(ref h) => unsafe {
                    libc::ftruncate(h.borrow_fd().as_raw_fd(), size.try_into().unwrap())
                },
                _ => {
                    // There is no `ftruncateat` so we need to get a new fd and truncate it.
                    let f = self
                        .open_inode(inode, libc::O_NONBLOCK | libc::O_RDWR)
                        .await?;
                    unsafe { libc::ftruncate(f.as_raw_fd(), size.try_into().unwrap()) }
                }
            };
            if res < 0 {
                return Err(io::Error::last_os_error().into());
            }
        }

        if set_attr.atime.is_some() || set_attr.mtime.is_some() {
            // POSIX utime() permission rules:
            // - utime(NULL): requires owner OR write permission
            // - utime(&times): requires owner only
            //
            // At FUSE level, we cannot reliably distinguish these cases because VFS
            // converts both to actual timestamps. We use a heuristic:
            // - If both nsec == 0 and timestamp is in the past: likely utime(&times)
            // - Otherwise: likely utime(NULL) which gets current time with nsec precision

            // SAFETY: libc::time with null pointer is a read-only syscall that always
            // succeeds and doesn't modify memory.
            let now = unsafe { libc::time(std::ptr::null_mut()) };

            // Heuristic: utime(&times) typically sets whole seconds (both nsec=0) to past times.
            // utime(NULL) sets current time which usually has non-zero nsec.
            // Both timestamps and both conditions must be satisfied to avoid false positives.
            let is_utime_times =
                if let (Some(atime_ts), Some(mtime_ts)) = (set_attr.atime, set_attr.mtime) {
                    (atime_ts.nsec == 0 && mtime_ts.nsec == 0)
                        && (atime_ts.sec < now && mtime_ts.sec < now)
                } else {
                    // If one is None, it's likely a specific update, treat as requiring ownership.
                    true
                };

            let st = stat_fd(&file, None)?;
            let uid = self.cfg.mapping.get_uid(req.uid);
            let gid = self.cfg.mapping.get_gid(req.gid);

            let is_owner = st.st_uid == uid;

            if !is_owner {
                if is_utime_times {
                    // utime(&times): only owner allowed
                    return Err(io::Error::from_raw_os_error(libc::EPERM).into());
                } else {
                    // utime(NULL): check for write permission
                    // Check user, group, and other permissions
                    // NOTE: This currently only checks the primary gid. A complete POSIX-compliant
                    // implementation should check all supplementary groups from req.groups if available.
                    // However, rfuse3::Request currently doesn't expose supplementary group information.
                    let has_user_write = st.st_uid == uid && st.st_mode & 0o200 != 0;
                    let has_group_write = st.st_gid == gid && st.st_mode & 0o020 != 0;
                    let has_other_write = st.st_mode & 0o002 != 0;

                    if !has_user_write && !has_group_write && !has_other_write {
                        return Err(io::Error::from_raw_os_error(libc::EPERM).into());
                    }
                }
            }
            let mut tvs: [libc::timespec; 2] = [
                libc::timespec {
                    tv_sec: 0,
                    tv_nsec: libc::UTIME_OMIT,
                },
                libc::timespec {
                    tv_sec: 0,
                    tv_nsec: libc::UTIME_OMIT,
                },
            ];
            if let Some(atime_ts) = set_attr.atime {
                tvs[0].tv_sec = atime_ts.sec;
                tvs[0].tv_nsec = atime_ts.nsec as i64;
            }
            if let Some(mtime_ts) = set_attr.mtime {
                tvs[1].tv_sec = mtime_ts.sec;
                tvs[1].tv_nsec = mtime_ts.nsec as i64;
            }

            // Safe because this doesn't modify any memory and we check the return value.
            let res = match data {
                Data::Handle(ref h) => unsafe {
                    libc::futimens(h.borrow_fd().as_raw_fd(), tvs.as_ptr())
                },
                Data::ProcPath(ref p) => unsafe {
                    libc::utimensat(self.proc_self_fd.as_raw_fd(), p.as_ptr(), tvs.as_ptr(), 0)
                },
            };
            if res < 0 {
                return Err(io::Error::last_os_error().into());
            }
        }

        // After any successful modification, re-stat the file to get fresh attributes.
        // Use `do_getattr` which correctly handles ID mapping.
        let (new_stat, _attr_timeout) = self.do_getattr(inode, fh).await?;
        // Crucially, return a ReplyAttr with a zero TTL.
        // This tells the kernel to invalidate its attribute cache for this inode immediately.
        // Subsequent `stat()` calls from clients will trigger a fresh `getattr` request.
        Ok(ReplyAttr {
            ttl: Duration::new(0, 0),
            attr: convert_stat64_to_file_attr(new_stat),
        })
    }

    /// read symbolic link.
    async fn readlink(&self, _req: Request, inode: Inode) -> Result<ReplyData> {
        // Safe because this is a constant value and a valid C string.
        let empty = unsafe { CStr::from_bytes_with_nul_unchecked(EMPTY_CSTR) };
        let mut buf = Vec::<u8>::with_capacity(libc::PATH_MAX as usize);
        let data = self.inode_map.get(inode).await?;

        let file = data.get_file()?;

        // Safe because this will only modify the contents of `buf` and we check the return value.
        let res = unsafe {
            libc::readlinkat(
                file.as_raw_fd(),
                empty.as_ptr(),
                buf.as_mut_ptr() as *mut libc::c_char,
                libc::PATH_MAX as usize,
            )
        };
        if res < 0 {
            return Err(io::Error::last_os_error().into());
        }

        // Safe because we trust the value returned by kernel.
        unsafe { buf.set_len(res as usize) };

        Ok(ReplyData {
            data: Bytes::from(buf),
        })
    }

    /// create a symbolic link.
    async fn symlink(
        &self,
        req: Request,
        parent: Inode,
        name: &OsStr,
        link: &OsStr,
    ) -> Result<ReplyEntry> {
        self.do_symlink_inner(req, parent, name, link, None, None)
            .await
    }

    /// create file node. Create a regular file, character device, block device, fifo or socket
    /// node. When creating file, most cases user only need to implement
    /// [`create`][Filesystem::create].
    async fn mknod(
        &self,
        req: Request,
        parent: Inode,
        name: &OsStr,
        mode: u32,
        rdev: u32,
    ) -> Result<ReplyEntry> {
        let name = osstr_to_cstr(name).unwrap();
        let name = name.as_ref();
        self.validate_path_component(name)?;

        let data = self.inode_map.get(parent).await?;
        let file = data.get_file()?;

        let res = {
            let (_uid, _gid) = set_creds(
                self.cfg.mapping.get_uid(req.uid),
                self.cfg.mapping.get_gid(req.gid),
            )?;

            // Safe because this doesn't modify any memory and we check the return value.
            unsafe {
                libc::mknodat(
                    file.as_raw_fd(),
                    name.as_ptr(),
                    (mode) as libc::mode_t,
                    u64::from(rdev),
                )
            }
        };
        if res < 0 {
            Err(io::Error::last_os_error().into())
        } else {
            self.do_lookup(parent, name).await
        }
    }

    /// create a directory.
    async fn mkdir(
        &self,
        req: Request,
        parent: Inode,
        name: &OsStr,
        mode: u32,
        umask: u32,
    ) -> Result<ReplyEntry> {
        self.do_mkdir_inner(req, parent, name, mode, umask, None, None)
            .await
    }

    /// remove a file.
    async fn unlink(&self, _req: Request, parent: Inode, name: &OsStr) -> Result<()> {
        let name = osstr_to_cstr(name).unwrap();
        let name = name.as_ref();
        self.validate_path_component(name)?;
        self.do_unlink(parent, name, 0).await.map_err(|e| e.into())
    }

    /// remove a directory.
    async fn rmdir(&self, _req: Request, parent: Inode, name: &OsStr) -> Result<()> {
        let name = osstr_to_cstr(name).unwrap();
        let name = name.as_ref();
        self.validate_path_component(name)?;
        self.do_unlink(parent, name, libc::AT_REMOVEDIR)
            .await
            .map_err(|e| e.into())
    }

    /// create a hard link.
    async fn link(
        &self,
        _req: Request,
        inode: Inode,
        new_parent: Inode,
        new_name: &OsStr,
    ) -> Result<ReplyEntry> {
        trace!(
            "passthrough: link: inode={}, new_parent={}, new_name={}",
            inode,
            new_parent,
            new_name.to_str().unwrap()
        );
        let newname = osstr_to_cstr(new_name).unwrap();
        let newname = newname.as_ref();
        self.validate_path_component(newname)?;

        trace!("link: trying to get inode {inode}");
        let data = self.inode_map.get(inode).await?;
        trace!("link: trying to get new parent {new_parent}");
        let new_inode = self.inode_map.get(new_parent).await?;
        let file = data.get_file()?;
        let new_file = new_inode.get_file()?;

        // Safe because this is a constant value and a valid C string.
        let empty = unsafe { CStr::from_bytes_with_nul_unchecked(EMPTY_CSTR) };

        // Safe because this doesn't modify any memory and we check the return value.
        let res = unsafe {
            libc::linkat(
                file.as_raw_fd(),
                empty.as_ptr(),
                new_file.as_raw_fd(),
                newname.as_ptr(),
                libc::AT_EMPTY_PATH,
            )
        };
        if res == 0 {
            trace!(
                "passthrough: link: inode={}, new_parent={}, new_name={}, res=0, trying to lookup",
                inode,
                new_parent,
                newname.to_str().unwrap()
            );
            self.do_lookup(new_parent, newname).await
        } else {
            trace!(
                "passthrough: link: inode={}, new_parent={}, new_name={}, res={}",
                inode,
                new_parent,
                newname.to_str().unwrap(),
                res
            );
            Err(io::Error::last_os_error().into())
        }
    }

    /// open a file. Open flags (with the exception of `O_CREAT`, `O_EXCL` and `O_NOCTTY`) are
    /// available in flags. Filesystem may store an arbitrary file handle (pointer, index, etc) in
    /// fh, and use this in other all other file operations (read, write, flush, release, fsync).
    /// Filesystem may also implement stateless file I/O and not store anything in fh. There are
    /// also some flags (`direct_io`, `keep_cache`) which the filesystem may set, to change the way
    /// the file is opened. A filesystem need not implement this method if it
    /// sets [`MountOptions::no_open_support`][rfuse3::MountOptions::no_open_support] and if the
    /// kernel supports `FUSE_NO_OPEN_SUPPORT`.
    ///
    /// # Notes:
    ///
    /// See `fuse_file_info` structure in
    /// [fuse_common.h](https://libfuse.github.io/doxygen/include_2fuse__common_8h_source.html) for
    /// more details.
    async fn open(&self, _req: Request, inode: Inode, flags: u32) -> Result<ReplyOpen> {
        if self.no_open.load(Ordering::Relaxed) {
            info!("fuse: open is not supported.");
            Err(enosys().into())
        } else {
            let re = self.do_open(inode, flags).await?;
            Ok(ReplyOpen {
                fh: re.0.unwrap(),
                flags: re.1.bits(),
            })
        }
    }

    /// read data. Read should send exactly the number of bytes requested except on EOF or error,
    /// otherwise the rest of the data will be substituted with zeroes. An exception to this is
    /// when the file has been opened in `direct_io` mode, in which case the return value of the
    /// read system call will reflect the return value of this operation. `fh` will contain the
    /// value set by the open method, or will be undefined if the open method didn't set any value.
    async fn read(
        &self,
        _req: Request,
        inode: Inode,
        fh: u64,
        offset: u64,
        size: u32,
    ) -> Result<ReplyData> {
        let data = self.get_data(fh, inode, libc::O_RDONLY).await?;
        let _guard = data.lock.lock().await;
        let raw_fd = data.borrow_fd().as_raw_fd();

        let mut buf = vec![0; size as usize];
        let file = &data.file;

        let res = if self.cfg.use_mmap {
            self.read_from_mmap(inode, offset, size as u64, file, buf.as_mut_slice())
                .await
                .ok()
        } else {
            None
        };

        match res {
            Some(bytes_read) => {
                if bytes_read < size as usize {
                    buf.truncate(bytes_read); // Adjust the buffer size for EOF
                }
            }
            None => {
                if offset > i64::MAX as u64 {
                    error!("read error: offset too large: {}", offset);
                    return Err(Errno::from(libc::EOVERFLOW));
                }
                const ALIGN: usize = 4096;
                let open_flags = data.get_flags().await;
                let ret = if (open_flags as i32 & libc::O_DIRECT) != 0 {
                    let mut aligned_buf = unsafe {
                        let layout = std::alloc::Layout::from_size_align(size as _, ALIGN).unwrap();
                        let ptr = std::alloc::alloc(layout);
                        if ptr.is_null() {
                            return Err(io::Error::from_raw_os_error(libc::ENOMEM).into());
                        }
                        Vec::from_raw_parts(ptr, size as _, size as _)
                    };
                    let ret = unsafe {
                        pread(
                            raw_fd as c_int,
                            aligned_buf.as_mut_ptr() as *mut libc::c_void,
                            size as size_t,
                            offset as off_t,
                        )
                    };

                    if ret >= 0 {
                        let bytes_read = ret as usize;
                        buf.as_mut_slice()[..bytes_read]
                            .copy_from_slice(&aligned_buf[..bytes_read]);
                    }
                    ret
                } else {
                    unsafe {
                        pread(
                            raw_fd as c_int,
                            buf.as_mut_ptr() as *mut libc::c_void,
                            size as size_t,
                            offset as off_t,
                        )
                    }
                };
                if ret < 0 {
                    let e = io::Error::last_os_error();
                    error!("read error: {e:?}");
                    error!(
                        "pread raw_fd={}, pointer={:p}, size={}, offset={}",
                        raw_fd,
                        buf.as_mut_ptr(),
                        size,
                        offset
                    );
                    return Err(e.into());
                } else {
                    let bytes_read = ret as usize;
                    buf.truncate(bytes_read);
                }
            }
        }

        Ok(ReplyData {
            data: Bytes::from(buf),
        })
    }

    /// write data. Write should return exactly the number of bytes requested except on error. An
    /// exception to this is when the file has been opened in `direct_io` mode, in which case the
    /// return value of the write system call will reflect the return value of this operation. `fh`
    /// will contain the value set by the open method, or will be undefined if the open method
    /// didn't set any value. When `write_flags` contains
    /// [`FUSE_WRITE_CACHE`][rfuse3::raw::flags::FUSE_WRITE_CACHE], means the write operation is a
    /// delay write.
    #[allow(clippy::too_many_arguments)]
    async fn write(
        &self,
        _req: Request,
        inode: Inode,
        fh: u64,
        offset: u64,
        data: &[u8],
        _write_flags: u32,
        flags: u32,
    ) -> Result<ReplyWrite> {
        let handle_data = self.get_data(fh, inode, libc::O_RDWR).await?;
        let file = &handle_data.file;
        let _guard = handle_data.lock.lock().await;
        let raw_fd = handle_data.borrow_fd().as_raw_fd();

        let res = if self.cfg.use_mmap {
            self.write_to_mmap(inode, offset, data, file).await.ok()
        } else {
            None
        };

        let ret = match res {
            Some(ret) => ret as isize,
            None => {
                let size = data.len();
                if offset > i64::MAX as u64 {
                    error!("write error: offset too large: {}", offset);
                    return Err(Errno::from(libc::EOVERFLOW));
                }
                self.check_fd_flags(&handle_data, raw_fd, flags).await?;
                let ret = unsafe {
                    libc::pwrite(
                        raw_fd as c_int,
                        data.as_ptr() as *const libc::c_void,
                        size as size_t,
                        offset as off_t,
                    )
                };
                if ret >= 0 {
                    ret
                } else {
                    let e = io::Error::last_os_error();
                    error!("write error: {e:?}");
                    error!(
                        "pwrite raw_fd={}, pointer={:p}, size={}, offset={}",
                        raw_fd,
                        data.as_ptr(),
                        size,
                        offset
                    );
                    return Err(Errno::from(e.raw_os_error().unwrap_or(-1)));
                }
            }
        };

        Ok(ReplyWrite {
            written: ret as u32,
        })
    }

    /// get filesystem statistics.
    async fn statfs(&self, _req: Request, inode: Inode) -> Result<ReplyStatFs> {
        let mut out = MaybeUninit::<libc::statvfs64>::zeroed();
        let data = self.inode_map.get(inode).await?;
        let file = data.get_file()?;

        // Safe because this will only modify `out` and we check the return value.
        let statfs: libc::statvfs64 =
            match unsafe { libc::fstatvfs64(file.as_raw_fd(), out.as_mut_ptr()) } {
                // Safe because the kernel guarantees that `out` has been initialized.
                0 => unsafe { out.assume_init() },
                _ => return Err(io::Error::last_os_error().into()),
            };

        Ok(
            // Populate the ReplyStatFs structure with the necessary information
            ReplyStatFs {
                blocks: statfs.f_blocks,
                bfree: statfs.f_bfree,
                bavail: statfs.f_bavail,
                files: statfs.f_files,
                ffree: statfs.f_ffree,
                bsize: statfs.f_bsize as u32,
                namelen: statfs.f_namemax as u32,
                frsize: statfs.f_frsize as u32,
            },
        )
    }

    /// release an open file. Release is called when there are no more references to an open file:
    /// all file descriptors are closed and all memory mappings are unmapped. For every open call
    /// there will be exactly one release call. The filesystem may reply with an error, but error
    /// values are not returned to `close()` or `munmap()` which triggered the release. `fh` will
    /// contain the value set by the open method, or will be undefined if the open method didn't
    /// set any value. `flags` will contain the same flags as for open. `flush` means flush the
    /// data or not when closing file.
    async fn release(
        &self,
        _req: Request,
        inode: Inode,
        fh: u64,
        _flags: u32,
        _lock_owner: u64,
        _flush: bool,
    ) -> Result<()> {
        if self.no_open.load(Ordering::Relaxed) {
            Err(enosys().into())
        } else {
            self.do_release(inode, fh).await.map_err(|e| e.into())
        }
    }

    /// synchronize file contents. If the `datasync` is true, then only the user data should be
    /// flushed, not the metadata.
    async fn fsync(&self, _req: Request, inode: Inode, fh: u64, datasync: bool) -> Result<()> {
        let data = self.get_data(fh, inode, libc::O_RDONLY).await?;
        let fd = data.borrow_fd();

        // Safe because this doesn't modify any memory and we check the return value.
        let res = unsafe {
            if datasync {
                libc::fdatasync(fd.as_raw_fd())
            } else {
                libc::fsync(fd.as_raw_fd())
            }
        };
        if res == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error().into())
        }
    }

    /// set an extended attribute.
    async fn setxattr(
        &self,
        _req: Request,
        inode: Inode,
        name: &OsStr,
        value: &[u8],
        flags: u32,
        _position: u32,
    ) -> Result<()> {
        if !self.cfg.xattr {
            return Err(enosys().into());
        }
        let name = osstr_to_cstr(name).unwrap();
        let name = name.as_ref();
        let data = self.inode_map.get(inode).await?;
        let file = data.get_file()?;
        let pathname = CString::new(format!("/proc/self/fd/{}", file.as_raw_fd()))
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        // The f{set,get,remove,list}xattr functions don't work on an fd opened with `O_PATH` so we
        // need to use the {set,get,remove,list}xattr variants.
        // Safe because this doesn't modify any memory and we check the return value.
        let res = unsafe {
            libc::setxattr(
                pathname.as_ptr(),
                name.as_ptr(),
                value.as_ptr() as *const libc::c_void,
                value.len(),
                flags as libc::c_int,
            )
        };
        if res == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error().into())
        }
    }

    /// Get an extended attribute. If `size` is too small, return `Err<ERANGE>`.
    /// Otherwise, use [`ReplyXAttr::Data`] to send the attribute data, or
    /// return an error.
    async fn getxattr(
        &self,
        _req: Request,
        inode: Inode,
        name: &OsStr,
        size: u32,
    ) -> Result<ReplyXAttr> {
        if !self.cfg.xattr {
            return Err(enosys().into());
        }
        let name =
            osstr_to_cstr(name).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let name = name.as_ref();
        let data = self.inode_map.get(inode).await?;
        let file = data.get_file()?;
        let mut buf = Vec::<u8>::with_capacity(size as usize);
        let pathname = CString::new(format!("/proc/self/fd/{}", file.as_raw_fd(),))
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        // The f{set,get,remove,list}xattr functions don't work on an fd opened with `O_PATH` so we
        // need to use the {set,get,remove,list}xattr variants.
        // Safe because this will only modify the contents of `buf`.
        let res = unsafe {
            libc::getxattr(
                pathname.as_ptr(),
                name.as_ptr(),
                buf.as_mut_ptr() as *mut libc::c_void,
                size as libc::size_t,
            )
        };
        if res < 0 {
            let e = io::Error::last_os_error();
            // error!("getxattr error: {e:?}");
            return Err(e.into());
        }

        if size == 0 {
            Ok(ReplyXAttr::Size(res as u32))
        } else {
            // Safe because we trust the value returned by kernel.
            unsafe { buf.set_len(res as usize) };
            Ok(ReplyXAttr::Data(Bytes::from(buf)))
        }
    }

    /// List extended attribute names.
    ///
    /// If `size` is too small, return `Err<ERANGE>`.  Otherwise, use
    /// [`ReplyXAttr::Data`] to send the attribute list, or return an error.
    async fn listxattr(&self, _req: Request, inode: Inode, size: u32) -> Result<ReplyXAttr> {
        if !self.cfg.xattr {
            return Err(enosys().into());
        }

        let data = self.inode_map.get(inode).await?;
        let file = data.get_file()?;
        let mut buf = Vec::<u8>::with_capacity(size as usize);
        let pathname = CString::new(format!("/proc/self/fd/{}", file.as_raw_fd()))
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        // The f{set,get,remove,list}xattr functions don't work on an fd opened with `O_PATH` so we
        // need to use the {set,get,remove,list}xattr variants.
        // Safe because this will only modify the contents of `buf`.
        let res = unsafe {
            libc::listxattr(
                pathname.as_ptr(),
                buf.as_mut_ptr() as *mut libc::c_char,
                size as libc::size_t,
            )
        };
        if res < 0 {
            let e = io::Error::last_os_error();
            // error!("listxattr error: {e:?}");
            return Err(e.into());
        }

        if size == 0 {
            Ok(ReplyXAttr::Size(res as u32))
        } else {
            // Safe because we trust the value returned by kernel.
            unsafe { buf.set_len(res as usize) };
            Ok(ReplyXAttr::Data(Bytes::from(buf)))
        }
    }

    /// remove an extended attribute.
    async fn removexattr(&self, _req: Request, inode: Inode, name: &OsStr) -> Result<()> {
        if !self.cfg.xattr {
            return Err(enosys().into());
        }
        let name = osstr_to_cstr(name).unwrap();
        let name = name.as_ref();
        let data = self.inode_map.get(inode).await?;
        let file = data.get_file()?;
        let pathname = CString::new(format!("/proc/self/fd/{}", file.as_raw_fd()))
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        // The f{set,get,remove,list}xattr functions don't work on an fd opened with `O_PATH` so we
        // need to use the {set,get,remove,list}xattr variants.
        // Safe because this doesn't modify any memory and we check the return value.
        let res = unsafe { libc::removexattr(pathname.as_ptr(), name.as_ptr()) };
        if res == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error().into())
        }
    }

    /// flush method. This is called on each `close()` of the opened file. Since file descriptors
    /// can be duplicated (`dup`, `dup2`, `fork`), for one open call there may be many flush calls.
    /// Filesystems shouldn't assume that flush will always be called after some writes, or that if
    /// will be called at all. `fh` will contain the value set by the open method, or will be
    /// undefined if the open method didn't set any value.
    ///
    /// # Notes:
    ///
    /// the name of the method is misleading, since (unlike fsync) the filesystem is not forced to
    /// flush pending writes. One reason to flush data, is if the filesystem wants to return write
    /// errors. If the filesystem supports file locking operations ([`setlk`][Filesystem::setlk],
    /// [`getlk`][Filesystem::getlk]) it should remove all locks belonging to `lock_owner`.
    async fn flush(&self, _req: Request, inode: Inode, fh: u64, _lock_owner: u64) -> Result<()> {
        if self.no_open.load(Ordering::Relaxed) {
            return Err(enosys().into());
        }

        let data = self.handle_map.get(fh, inode).await?;
        trace!("flush: data.inode={}", data.inode);

        // Since this method is called whenever an fd is closed in the client, we can emulate that
        // behavior by doing the same thing (dup-ing the fd and then immediately closing it). Safe
        // because this doesn't modify any memory and we check the return values.
        unsafe {
            let newfd = libc::dup(data.borrow_fd().as_raw_fd());
            if newfd < 0 {
                return Err(io::Error::last_os_error().into());
            }

            if libc::close(newfd) < 0 {
                Err(io::Error::last_os_error().into())
            } else {
                Ok(())
            }
        }
        // if self.no_open.load(Ordering::Acquire) {
        //         return Err(enosys().into());
        //     }

        // let data = self.handle_map.get(fh, inode).await?;

        // // std flush impl
        // unsafe {
        //     let fd = data.borrow_fd().as_raw_fd();
        //     if libc::fsync(fd) < 0 {
        //         let err = io::Error::last_os_error();
        //         error!("Failed to fsync file descriptor {}: {}", fd, err);
        //         return Err(err.into());
        //     }
        // }
        // Ok(())
    }

    /// open a directory. Filesystem may store an arbitrary file handle (pointer, index, etc) in
    /// `fh`, and use this in other all other directory stream operations
    /// ([`readdir`][Filesystem::readdir], [`releasedir`][Filesystem::releasedir],
    /// [`fsyncdir`][Filesystem::fsyncdir]). Filesystem may also implement stateless directory
    /// I/O and not store anything in `fh`.  A file system need not implement this method if it
    /// sets [`MountOptions::no_open_dir_support`][rfuse3::MountOptions::no_open_dir_support] and
    /// if the kernel supports `FUSE_NO_OPENDIR_SUPPORT`.
    async fn opendir(&self, _req: Request, inode: Inode, flags: u32) -> Result<ReplyOpen> {
        if self.no_opendir.load(Ordering::Relaxed) {
            info!("fuse: opendir is not supported.");
            Err(enosys().into())
        } else {
            let t = self
                .do_open(inode, flags | (libc::O_DIRECTORY as u32))
                .await?;
            let fd = t.0.unwrap();
            Ok(ReplyOpen {
                fh: fd,
                flags: t.1.bits(),
            })
        }
    }

    /// read directory. `offset` is used to track the offset of the directory entries. `fh` will
    /// contain the value set by the [`opendir`][Filesystem::opendir] method, or will be
    /// undefined if the [`opendir`][Filesystem::opendir] method didn't set any value.
    async fn readdir<'a>(
        &'a self,
        _req: Request,
        parent: Inode,
        fh: u64,
        offset: i64,
    ) -> Result<
        ReplyDirectory<
            impl futures_util::stream::Stream<Item = Result<DirectoryEntry>> + Send + 'a,
        >,
    > {
        if self.no_readdir.load(Ordering::Relaxed) {
            return Err(enosys().into());
        }
        let mut entry_list = Vec::new();
        self.do_readdir(parent, fh, offset as u64, &mut entry_list)
            .await?;
        Ok(ReplyDirectory {
            entries: stream::iter(entry_list),
        })
    }

    /// read directory entries, but with their attribute, like [`readdir`][Filesystem::readdir]
    /// + [`lookup`][Filesystem::lookup] at the same time.
    async fn readdirplus<'a>(
        &'a self,
        _req: Request,
        parent: Inode,
        fh: u64,
        offset: u64,
        _lock_owner: u64,
    ) -> Result<
        ReplyDirectoryPlus<
            impl futures_util::stream::Stream<Item = Result<DirectoryEntryPlus>> + Send + 'a,
        >,
    > {
        if self.no_readdir.load(Ordering::Relaxed) {
            return Err(enosys().into());
        }
        let mut entry_list = Vec::new();
        self.do_readdirplus(parent, fh, offset, &mut entry_list)
            .await?;
        Ok(ReplyDirectoryPlus {
            entries: stream::iter(entry_list),
        })
    }

    /// release an open directory. For every [`opendir`][Filesystem::opendir] call there will
    /// be exactly one `releasedir` call. `fh` will contain the value set by the
    /// [`opendir`][Filesystem::opendir] method, or will be undefined if the
    /// [`opendir`][Filesystem::opendir] method didn't set any value.
    async fn releasedir(&self, _req: Request, inode: Inode, fh: u64, _flags: u32) -> Result<()> {
        if self.no_opendir.load(Ordering::Relaxed) {
            info!("fuse: releasedir is not supported.");
            Err(io::Error::from_raw_os_error(libc::ENOSYS).into())
        } else {
            self.do_release(inode, fh).await.map_err(|e| e.into())
        }
    }

    /// synchronize directory contents. If the `datasync` is true, then only the directory contents
    /// should be flushed, not the metadata. `fh` will contain the value set by the
    /// [`opendir`][Filesystem::opendir] method, or will be undefined if the
    /// [`opendir`][Filesystem::opendir] method didn't set any value.
    async fn fsyncdir(&self, req: Request, inode: Inode, fh: u64, datasync: bool) -> Result<()> {
        self.fsync(req, inode, fh, datasync).await
    }

    /// check file access permissions. This will be called for the `access()` system call. If the
    /// `default_permissions` mount option is given, this method is not be called. This method is
    /// not called under Linux kernel versions 2.4.x.
    async fn access(&self, req: Request, inode: Inode, mask: u32) -> Result<()> {
        let data = self.inode_map.get(inode).await?;
        let st = stat_fd(&data.get_file()?, None)?;
        let mode = mask as i32 & (libc::R_OK | libc::W_OK | libc::X_OK);

        let uid = self.cfg.mapping.get_uid(req.uid);
        let gid = self.cfg.mapping.get_gid(req.gid);

        if mode == libc::F_OK {
            // The file exists since we were able to call `stat(2)` on it.
            return Ok(());
        }

        if (mode & libc::R_OK) != 0
            && uid != 0
            && (st.st_uid != uid || st.st_mode & 0o400 == 0)
            && (st.st_gid != gid || st.st_mode & 0o040 == 0)
            && st.st_mode & 0o004 == 0
        {
            return Err(io::Error::from_raw_os_error(libc::EACCES).into());
        }

        if (mode & libc::W_OK) != 0
            && uid != 0
            && (st.st_uid != uid || st.st_mode & 0o200 == 0)
            && (st.st_gid != gid || st.st_mode & 0o020 == 0)
            && st.st_mode & 0o002 == 0
        {
            return Err(io::Error::from_raw_os_error(libc::EACCES).into());
        }

        // root can only execute something if it is executable by one of the owner, the group, or
        // everyone.
        if (mode & libc::X_OK) != 0
            && (uid != 0 || st.st_mode & 0o111 == 0)
            && (st.st_uid != uid || st.st_mode & 0o100 == 0)
            && (st.st_gid != gid || st.st_mode & 0o010 == 0)
            && st.st_mode & 0o001 == 0
        {
            return Err(io::Error::from_raw_os_error(libc::EACCES).into());
        }

        Ok(())
    }

    /// create and open a file. If the file does not exist, first create it with the specified
    /// mode, and then open it. Open flags (with the exception of `O_NOCTTY`) are available in
    /// flags. Filesystem may store an arbitrary file handle (pointer, index, etc) in `fh`, and use
    /// this in other all other file operations ([`read`][Filesystem::read],
    /// [`write`][Filesystem::write], [`flush`][Filesystem::flush],
    /// [`release`][Filesystem::release], [`fsync`][Filesystem::fsync]). There are also some flags
    /// (`direct_io`, `keep_cache`) which the filesystem may set, to change the way the file is
    /// opened. If this method is not implemented or under Linux kernel versions earlier than
    /// 2.6.15, the [`mknod`][Filesystem::mknod] and [`open`][Filesystem::open] methods will be
    /// called instead.
    ///
    /// # Notes:
    ///
    /// See `fuse_file_info` structure in
    /// [fuse_common.h](https://libfuse.github.io/doxygen/include_2fuse__common_8h_source.html) for
    /// more details.
    async fn create(
        &self,
        req: Request,
        parent: Inode,
        name: &OsStr,
        mode: u32,
        flags: u32,
    ) -> Result<ReplyCreated> {
        self.do_create_inner(req, parent, name, mode, flags, None, None)
            .await
    }

    /// handle interrupt. When a operation is interrupted, an interrupt request will send to fuse
    /// server with the unique id of the operation.
    async fn interrupt(&self, _req: Request, _unique: u64) -> Result<()> {
        Ok(())
    }

    /// forget more than one inode. This is a batch version [`forget`][Filesystem::forget]
    async fn batch_forget(&self, _req: Request, inodes: &[(Inode, u64)]) {
        let mut inodes_w = self.inode_map.inodes.write().await;

        for i in inodes {
            self.forget_one(&mut inodes_w, i.0, i.1).await;
        }
    }

    /// allocate space for an open file. This function ensures that required space is allocated for
    /// specified file.
    ///
    /// # Notes:
    ///
    /// more information about `fallocate`, please see **`man 2 fallocate`**
    async fn fallocate(
        &self,
        _req: Request,
        inode: Inode,
        fh: u64,
        offset: u64,
        length: u64,
        mode: u32,
    ) -> Result<()> {
        // Let the Arc<HandleData> in scope, otherwise fd may get invalid.
        let data = self.get_data(fh, inode, libc::O_RDWR).await?;
        let fd = data.borrow_fd();

        //  if self.seal_size.load().await {
        //      let st = stat_fd(&fd, None)?;
        //      self.seal_size_check(
        //          Opcode::Fallocate,
        //          st.st_size as u64,
        //          offset,
        //          length,
        //          mode as i32,
        //      )?;
        //  }

        // Safe because this doesn't modify any memory and we check the return value.
        let res = unsafe {
            libc::fallocate64(
                fd.as_raw_fd(),
                mode as libc::c_int,
                offset as libc::off64_t,
                length as libc::off64_t,
            )
        };

        if res == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error().into())
        }
    }

    /// rename a file or directory.
    async fn rename(
        &self,
        _req: Request,
        parent: Inode,
        name: &OsStr,
        new_parent: Inode,
        new_name: &OsStr,
    ) -> Result<()> {
        let oldname = osstr_to_cstr(name).unwrap();
        let oldname = oldname.as_ref();
        let newname = osstr_to_cstr(new_name).unwrap();
        let newname = newname.as_ref();
        self.validate_path_component(oldname)?;
        self.validate_path_component(newname)?;

        // Check if new_name exists and is a whiteout file
        let new_parent_data = self.inode_map.get(new_parent).await?;
        let new_parent_file = new_parent_data.get_file()?;

        // Try to lookup newname to see if it exists
        // Check if new_name exists and is a whiteout file
        let mut st = std::mem::MaybeUninit::<libc::stat>::uninit();
        let res = unsafe {
            libc::fstatat(
                new_parent_file.as_raw_fd(),
                newname.as_ptr(),
                st.as_mut_ptr(),
                libc::AT_SYMLINK_NOFOLLOW,
            )
        };

        if res == 0 {
            // If file exists, check if it's a whiteout file
            let st = unsafe { st.assume_init() };
            if (st.st_mode & libc::S_IFMT) == libc::S_IFCHR && st.st_rdev == 0 {
                // It's a whiteout file, delete it
                let unlink_res =
                    unsafe { libc::unlinkat(new_parent_file.as_raw_fd(), newname.as_ptr(), 0) };
                if unlink_res < 0 {
                    return Err(io::Error::last_os_error().into());
                }
            }
        } else {
            let err = io::Error::last_os_error();
            if err.raw_os_error() != Some(libc::ENOENT) {
                return Err(err.into());
            }
        }

        let old_inode = self.inode_map.get(parent).await?;
        let new_inode = self.inode_map.get(new_parent).await?;
        let old_file = old_inode.get_file()?;
        let new_file = new_inode.get_file()?;

        //TODO: Switch to libc::renameat2 -> libc::renameat2(olddirfd, oldpath, newdirfd, newpath, flags)
        let res = unsafe {
            libc::renameat(
                old_file.as_raw_fd(),
                oldname.as_ptr(),
                new_file.as_raw_fd(),
                newname.as_ptr(),
            )
        };

        if res == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error().into())
        }
    }

    /// rename a file or directory with flags.
    async fn rename2(
        &self,
        _req: Request,
        parent: Inode,
        name: &OsStr,
        new_parent: Inode,
        new_name: &OsStr,
        flags: u32,
    ) -> Result<()> {
        let oldname = osstr_to_cstr(name).unwrap();
        let oldname = oldname.as_ref();
        let newname = osstr_to_cstr(new_name).unwrap();
        let newname = newname.as_ref();
        self.validate_path_component(oldname)?;
        self.validate_path_component(newname)?;

        let old_inode = self.inode_map.get(parent).await?;
        let new_inode = self.inode_map.get(new_parent).await?;
        let old_file = old_inode.get_file()?;
        let new_file = new_inode.get_file()?;
        //TODO: Switch to libc::renameat2 -> libc::renameat2(olddirfd, oldpath, newdirfd, newpath, flags)
        let res = unsafe {
            libc::renameat2(
                old_file.as_raw_fd(),
                oldname.as_ptr(),
                new_file.as_raw_fd(),
                newname.as_ptr(),
                flags,
            )
        };

        if res == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error().into())
        }
    }

    /// find next data or hole after the specified offset.
    async fn lseek(
        &self,
        _req: Request,
        inode: Inode,
        fh: u64,
        offset: u64,
        whence: u32,
    ) -> Result<ReplyLSeek> {
        // Let the Arc<HandleData> in scope, otherwise fd may get invalid.
        let data = self.handle_map.get(fh, inode).await?;

        // Check file type to determine appropriate lseek handling
        let st = stat_fd(data.get_file(), None)?;
        let is_dir = (st.st_mode & libc::S_IFMT) == libc::S_IFDIR;

        if is_dir {
            // Directory special handling: support SEEK_SET and SEEK_CUR with bounds checks.
            // Acquire the lock to get exclusive access
            let (_guard, file) = data.get_file_mut().await;

            // Handle directory lseek operations according to POSIX standard
            // This enables seekdir/telldir functionality on directories
            match whence {
                // SEEK_SET: set directory offset to an absolute value
                x if x == libc::SEEK_SET as u32 => {
                    // Validate offset bounds to prevent overflow
                    // Directory offsets should not exceed i64::MAX
                    if offset > i64::MAX as u64 {
                        return Err(io::Error::from_raw_os_error(libc::EINVAL).into());
                    }

                    // Perform the seek operation using libc::lseek64
                    // This directly manipulates the file descriptor's position
                    let res = unsafe {
                        libc::lseek64(file.as_raw_fd(), offset as libc::off64_t, libc::SEEK_SET)
                    };
                    if res < 0 {
                        return Err(io::Error::last_os_error().into());
                    }
                    Ok(ReplyLSeek { offset: res as u64 })
                }
                // SEEK_CUR: move relative to current directory offset
                x if x == libc::SEEK_CUR as u32 => {
                    // Get current position using libc::lseek64 with offset 0
                    let cur = unsafe { libc::lseek64(file.as_raw_fd(), 0, libc::SEEK_CUR) };
                    if cur < 0 {
                        return Err(io::Error::last_os_error().into());
                    }
                    let current = cur as u64;

                    // Compute new offset safely to prevent arithmetic overflow
                    if let Some(new_offset) = current.checked_add(offset) {
                        // Ensure the new offset is within valid bounds
                        if new_offset > i64::MAX as u64 {
                            return Err(io::Error::from_raw_os_error(libc::EINVAL).into());
                        }
                        // Set the new offset using libc::lseek64
                        let res = unsafe {
                            libc::lseek64(
                                file.as_raw_fd(),
                                new_offset as libc::off64_t,
                                libc::SEEK_SET,
                            )
                        };
                        if res < 0 {
                            return Err(io::Error::last_os_error().into());
                        }
                        Ok(ReplyLSeek { offset: new_offset })
                    } else {
                        Err(io::Error::from_raw_os_error(libc::EINVAL).into())
                    }
                }
                // Other whence values are invalid for directories (e.g., SEEK_END)
                _ => Err(io::Error::from_raw_os_error(libc::EINVAL).into()),
            }
        } else {
            // File seek handling for non-directory files
            // Acquire the lock to get exclusive access, otherwise it may break do_readdir().
            let (_guard, file) = data.get_file_mut().await;

            // Safe because this doesn't modify any memory and we check the return value.
            // Use 64-bit seek for regular files to match kernel offsets
            let res = unsafe {
                libc::lseek64(
                    file.as_raw_fd(),
                    offset as libc::off64_t,
                    whence as libc::c_int,
                )
            };
            if res < 0 {
                Err(io::Error::last_os_error().into())
            } else {
                Ok(ReplyLSeek { offset: res as u64 })
            }
        }
    }

    /// Copy a range of data from one file to another using the copy_file_range system call.
    /// This can improve performance by reducing data copying between userspace and kernel.
    #[allow(clippy::too_many_arguments)]
    async fn copy_file_range(
        &self,
        _req: Request,
        inode_in: Inode,
        fh_in: u64,
        offset_in: u64,
        inode_out: Inode,
        fh_out: u64,
        offset_out: u64,
        length: u64,
        flags: u64,
    ) -> Result<ReplyCopyFileRange> {
        // Get the handle data for both source and destination files
        let data_in = self.handle_map.get(fh_in, inode_in).await?;
        let data_out = self.handle_map.get(fh_out, inode_out).await?;

        // Get file descriptors
        let fd_in = data_in.borrow_fd().as_raw_fd();
        let fd_out = data_out.borrow_fd().as_raw_fd();

        // Validate and reject unsupported flags
        // Linux copy_file_range currently doesn't define any flags (should be 0)
        if flags != 0 {
            return Err(io::Error::from_raw_os_error(libc::EINVAL).into());
        }

        // Convert offsets to i64, checking for overflow (offsets > i64::MAX would wrap to negative)
        let mut off_in: i64 = offset_in
            .try_into()
            .map_err(|_| io::Error::from_raw_os_error(libc::EINVAL))?;
        let mut off_out: i64 = offset_out
            .try_into()
            .map_err(|_| io::Error::from_raw_os_error(libc::EINVAL))?;

        // Convert length to usize, checking for overflow on 32-bit systems
        let len: usize = length
            .try_into()
            .map_err(|_| io::Error::from_raw_os_error(libc::EINVAL))?;

        // SAFETY: copy_file_range reads from fd_in and writes to fd_out. We pass valid
        // file descriptors and pointers to offset values. The syscall updates the offset
        // pointers to reflect the new positions after the copy, but doesn't modify the
        // file descriptor positions themselves (when offsets are non-NULL).
        let res = unsafe {
            libc::copy_file_range(
                fd_in,
                &mut off_in as *mut i64, // Pass offset pointer directly
                fd_out,
                &mut off_out as *mut i64, // Pass offset pointer directly
                len,
                0, // flags (must be 0, already validated above)
            )
        };

        if res < 0 {
            Err(io::Error::last_os_error().into())
        } else {
            // res is guaranteed >= 0 here, safe to cast to usize then u64
            Ok(ReplyCopyFileRange {
                copied: res as usize as u64,
            })
        }
    }
}

/// trim all trailing nul terminators.
pub fn bytes_to_cstr(buf: &[u8]) -> Result<&CStr> {
    // There might be multiple 0s at the end of buf, find & use the first one and trim other zeros.
    match buf.iter().position(|x| *x == 0) {
        // Convert to a `CStr` so that we can drop the '\0' byte at the end and make sure
        // there are no interior '\0' bytes.
        Some(pos) => CStr::from_bytes_with_nul(&buf[0..=pos]).map_err(|_| Errno::from(5)),
        None => {
            // Invalid input, just call CStr::from_bytes_with_nul() for suitable error code
            CStr::from_bytes_with_nul(buf).map_err(|_| Errno::from(5))
        }
    }
}
