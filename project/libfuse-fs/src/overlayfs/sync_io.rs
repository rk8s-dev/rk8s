// Copyright (C) 2023 Ant Group. All rights reserved.
//  2024 From [fuse_backend_rs](https://github.com/cloud-hypervisor/fuse-backend-rs) 
// SPDX-License-Identifier: Apache-2.0

use super::*;
use std::ffi::CStr;
use std::io::Result;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use fuse_backend_rs::abi::fuse_abi::{stat64, statvfs64, CreateIn};
use fuse_backend_rs::api::filesystem::{
    Context, DirEntry, Entry, FileSystem, FsOptions, GetxattrReply, ListxattrReply, OpenOptions,
    SetattrValid, ZeroCopyReader, ZeroCopyWriter,
};

use libc;
use std::io::{Error, ErrorKind};

impl FileSystem for OverlayFs {
    type Inode = Inode;
    type Handle = Handle;

    fn init(&self, capable: FsOptions) -> Result<FsOptions> {
        // use vfs' negotiated capability if imported
        // other wise, do our own negotiation
        let mut opts = FsOptions::DO_READDIRPLUS | FsOptions::READDIRPLUS_AUTO;

        if self.config.do_import {
            self.import()?;
        }

        if (!self.config.do_import || self.config.writeback)
            && capable.contains(FsOptions::WRITEBACK_CACHE)
        {
            opts |= FsOptions::WRITEBACK_CACHE;
            self.writeback.store(true, Ordering::Relaxed);
        }

        if (!self.config.do_import || self.config.no_open)
            && capable.contains(FsOptions::ZERO_MESSAGE_OPEN)
        {
            opts |= FsOptions::ZERO_MESSAGE_OPEN;
            opts.remove(FsOptions::ATOMIC_O_TRUNC);
            self.no_open.store(true, Ordering::Relaxed);
        }

        if (!self.config.do_import || self.config.no_opendir)
            && capable.contains(FsOptions::ZERO_MESSAGE_OPENDIR)
        {
            opts |= FsOptions::ZERO_MESSAGE_OPENDIR;
            self.no_opendir.store(true, Ordering::Relaxed);
        }

        if (!self.config.do_import || self.config.killpriv_v2)
            && capable.contains(FsOptions::HANDLE_KILLPRIV_V2)
        {
            opts |= FsOptions::HANDLE_KILLPRIV_V2;
            self.killpriv_v2.store(true, Ordering::Relaxed);
        }

        if self.config.perfile_dax && capable.contains(FsOptions::PERFILE_DAX) {
            opts |= FsOptions::PERFILE_DAX;
            self.perfile_dax.store(true, Ordering::Relaxed);
        }

        Ok(opts)
    }

    fn destroy(&self) {}

    fn statfs(&self, ctx: &Context, inode: Inode) -> Result<statvfs64> {
        trace!("STATFS: inode: {}\n", inode);
        self.do_statvfs(ctx, inode)
    }

    fn lookup(&self, ctx: &Context, parent: Inode, name: &CStr) -> Result<Entry> {
        let tmp = name.to_string_lossy().to_string();
        trace!("LOOKUP: parent: {}, name: {}\n", parent, tmp);
        let result = self.do_lookup(ctx, parent, tmp.as_str());
        if result.is_ok() {
            trace!("LOOKUP result: {:?}", result.as_ref().unwrap());
        }
        //self.debug_print_all_inodes();
        result
    }

    fn forget(&self, _ctx: &Context, inode: Inode, count: u64) {
        trace!("FORGET: inode: {}, count: {}\n", inode, count);
        self.forget_one(inode, count);
        //self.debug_print_all_inodes();
    }

    fn batch_forget(&self, _ctx: &Context, requests: Vec<(Inode, u64)>) {
        trace!("BATCH_FORGET: requests: {:?}\n", requests);
        for (inode, count) in requests {
            self.forget_one(inode, count);
        }
    }

    fn opendir(
        &self,
        ctx: &Context,
        inode: Inode,
        _flags: u32,
    ) -> Result<(Option<Handle>, OpenOptions)> {
        trace!("OPENDIR: inode: {}\n", inode);
        if self.no_opendir.load(Ordering::Relaxed) {
            info!("fuse: opendir is not supported.");
            return Err(Error::from_raw_os_error(libc::ENOSYS));
        }

        let mut opts = OpenOptions::empty();

        if let CachePolicy::Always = self.config.cache_policy {
            opts |= OpenOptions::KEEP_CACHE;
        }

        // lookup node
        let node = self.lookup_node(ctx, inode, ".")?;

        if node.whiteout.load(Ordering::Relaxed) {
            return Err(Error::from_raw_os_error(libc::ENOENT));
        }

        let st = node.stat64(ctx)?;
        if !utils::is_dir(st) {
            return Err(Error::from_raw_os_error(libc::ENOTDIR));
        }

        let handle = self.next_handle.fetch_add(1, Ordering::Relaxed);

        self.handles.lock().unwrap().insert(
            handle,
            Arc::new(HandleData {
                node: Arc::clone(&node),
                real_handle: None,
            }),
        );

        Ok((Some(handle), opts))
    }

    fn releasedir(&self, _ctx: &Context, inode: Inode, _flags: u32, handle: Handle) -> Result<()> {
        trace!("RELEASEDIR: inode: {}, handle: {}\n", inode, handle);
        if self.no_opendir.load(Ordering::Relaxed) {
            info!("fuse: releasedir is not supported.");
            return Err(Error::from_raw_os_error(libc::ENOSYS));
        }

        self.handles.lock().unwrap().remove(&handle);

        Ok(())
    }

    // for mkdir or create file
    // 1. lookup name, if exists and not whiteout, return EEXIST
    // 2. not exists and no whiteout, copy up parent node, ususally  a mkdir on upper layer would do the work
    // 3. find whiteout, if whiteout in upper layer, should set opaque. if in lower layer, just mkdir?
    fn mkdir(
        &self,
        ctx: &Context,
        parent: Inode,
        name: &CStr,
        mode: u32,
        umask: u32,
    ) -> Result<Entry> {
        let sname = name.to_string_lossy().to_string();

        trace!("MKDIR: parent: {}, name: {}\n", parent, sname);

        // no entry or whiteout
        let pnode = self.lookup_node(ctx, parent, "")?;
        if pnode.whiteout.load(Ordering::Relaxed) {
            return Err(Error::from_raw_os_error(libc::ENOENT));
        }

        self.do_mkdir(ctx, &pnode, sname.as_str(), mode, umask)?;
        let entry = self.do_lookup(ctx, parent, sname.as_str());
        entry
    }

    fn rmdir(&self, ctx: &Context, parent: Inode, name: &CStr) -> Result<()> {
        trace!(
            "RMDIR: parent: {}, name: {}\n",
            parent,
            name.to_string_lossy()
        );
        self.do_rm(ctx, parent, name, true)
    }

    fn readdir(
        &self,
        ctx: &Context,
        inode: Inode,
        handle: Handle,
        size: u32,
        offset: u64,
        add_entry: &mut dyn FnMut(DirEntry) -> Result<usize>,
    ) -> Result<()> {
        trace!("READDIR: inode: {}, handle: {}\n", inode, handle);
        if self.config.no_readdir {
            info!("fuse: readdir is not supported.");
            return Ok(());
        }
        self.do_readdir(ctx, inode, handle, size, offset, false, &mut |dir_entry,
                                                                       _|
         -> Result<
            usize,
        > {
            add_entry(dir_entry)
        })
    }

    fn readdirplus(
        &self,
        ctx: &Context,
        inode: Inode,
        handle: Handle,
        size: u32,
        offset: u64,
        add_entry: &mut dyn FnMut(DirEntry, Entry) -> Result<usize>,
    ) -> Result<()> {
        trace!("READDIRPLUS: inode: {}, handle: {}\n", inode, handle);
        if self.config.no_readdir {
            info!("fuse: readdirplus is not supported.");
            return Ok(());
        }
        self.do_readdir(ctx, inode, handle, size, offset, true, &mut |dir_entry,
                                                                      entry|
         -> Result<
            usize,
        > {
            match entry {
                Some(e) => add_entry(dir_entry, e),
                None => Err(Error::from_raw_os_error(libc::ENOENT)),
            }
        })
    }

    fn open(
        &self,
        ctx: &Context,
        inode: Inode,
        flags: u32,
        fuse_flags: u32,
    ) -> Result<(Option<Handle>, OpenOptions, Option<u32>)> {
        // open assume file always exist
        trace!("OPEN: inode: {}, flags: {}\n", inode, flags);
        if self.no_open.load(Ordering::Relaxed) {
            info!("fuse: open is not supported.");
            return Err(Error::from_raw_os_error(libc::ENOSYS));
        }

        let readonly: bool = flags
            & (libc::O_APPEND | libc::O_CREAT | libc::O_TRUNC | libc::O_RDWR | libc::O_WRONLY)
                as u32
            == 0;
        // toggle flags
        let mut flags: i32 = flags as i32;

        flags |= libc::O_NOFOLLOW;

        if self.config.writeback {
            if flags & libc::O_ACCMODE == libc::O_WRONLY {
                flags &= !libc::O_ACCMODE;
                flags |= libc::O_RDWR;
            }

            if flags & libc::O_APPEND != 0 {
                flags &= !libc::O_APPEND;
            }
        }
        // lookup node
        let node = self.lookup_node(ctx, inode, "")?;

        // whiteout node
        if node.whiteout.load(Ordering::Relaxed) {
            return Err(Error::from_raw_os_error(libc::ENOENT));
        }

        if !readonly {
            // copy up to upper layer
            self.copy_node_up(ctx, Arc::clone(&node))?;
        }

        // assign a handle in overlayfs and open it
        let (_l, h, _) = node.open(ctx, flags as u32, fuse_flags)?;
        match h {
            None => Err(Error::from_raw_os_error(libc::ENOENT)),
            Some(handle) => {
                let hd = self.next_handle.fetch_add(1, Ordering::Relaxed);
                let (layer, in_upper_layer, inode) = node.first_layer_inode();
                let handle_data = HandleData {
                    node: Arc::clone(&node),
                    real_handle: Some(RealHandle {
                        layer,
                        in_upper_layer,
                        inode,
                        handle: AtomicU64::new(handle),
                    }),
                };

                self.handles
                    .lock()
                    .unwrap()
                    .insert(hd, Arc::new(handle_data));

                let mut opts = OpenOptions::empty();
                match self.config.cache_policy {
                    CachePolicy::Never => opts |= OpenOptions::DIRECT_IO,
                    CachePolicy::Always => opts |= OpenOptions::KEEP_CACHE,
                    _ => {}
                }

                trace!("OPEN: returning handle: {}", hd);

                Ok((Some(hd), opts, None))
            }
        }
    }

    fn release(
        &self,
        ctx: &Context,
        _inode: Inode,
        flags: u32,
        handle: Handle,
        flush: bool,
        flock_release: bool,
        lock_owner: Option<u64>,
    ) -> Result<()> {
        trace!(
            "RELEASE: inode: {}, flags: {}, handle: {}, flush: {}, flock_release: {}, lock_owner: {:?}\n",
            _inode,
            flags,
            handle,
            flush,
            flock_release,
            lock_owner
        );

        if self.no_open.load(Ordering::Relaxed) {
            info!("fuse: release is not supported.");
            return Err(Error::from_raw_os_error(libc::ENOSYS));
        }

        if let Some(hd) = self.handles.lock().unwrap().get(&handle) {
            let rh = if let Some(ref h) = hd.real_handle {
                h
            } else {
                return Err(Error::new(ErrorKind::Other, "no handle"));
            };
            let real_handle = rh.handle.load(Ordering::Relaxed);
            let real_inode = rh.inode;
            rh.layer.release(
                ctx,
                real_inode,
                flags,
                real_handle,
                flush,
                flock_release,
                lock_owner,
            )?;
        }

        self.handles.lock().unwrap().remove(&handle);

        Ok(())
    }

    fn create(
        &self,
        ctx: &Context,
        parent: Inode,
        name: &CStr,
        args: CreateIn,
    ) -> Result<(Entry, Option<Handle>, OpenOptions, Option<u32>)> {
        let sname = name.to_string_lossy().to_string();
        trace!("CREATE: parent: {}, name: {}\n", parent, sname);

        // Parent doesn't exist.
        let pnode = self.lookup_node(ctx, parent, "")?;
        if pnode.whiteout.load(Ordering::Relaxed) {
            return Err(Error::from_raw_os_error(libc::ENOENT));
        }

        let mut hargs = args;
        let mut flags: i32 = args.flags as i32;
        flags |= libc::O_NOFOLLOW;
        flags &= !libc::O_DIRECT;
        if self.config.writeback {
            if flags & libc::O_ACCMODE == libc::O_WRONLY {
                flags &= !libc::O_ACCMODE;
                flags |= libc::O_RDWR;
            }

            if flags & libc::O_APPEND != 0 {
                flags &= !libc::O_APPEND;
            }
        }
        hargs.flags = flags as u32;

        let final_handle = self.do_create(ctx, &pnode, sname.as_str(), hargs)?;
        let entry = self.do_lookup(ctx, parent, sname.as_str())?;

        let mut opts = OpenOptions::empty();
        match self.config.cache_policy {
            CachePolicy::Never => opts |= OpenOptions::DIRECT_IO,
            CachePolicy::Always => opts |= OpenOptions::KEEP_CACHE,
            _ => {}
        }

        Ok((entry, final_handle, opts, None))
    }

    fn unlink(&self, ctx: &Context, parent: Inode, name: &CStr) -> Result<()> {
        trace!(
            "UNLINK: parent: {}, name: {}\n",
            parent,
            name.to_string_lossy()
        );
        self.do_rm(ctx, parent, name, false)
    }

    fn read(
        &self,
        ctx: &Context,
        inode: Inode,
        handle: Handle,
        w: &mut dyn ZeroCopyWriter,
        size: u32,
        offset: u64,
        lock_owner: Option<u64>,
        flags: u32,
    ) -> Result<usize> {
        trace!(
            "READ: inode: {}, handle: {}, size: {}, offset: {}, lock_owner: {:?}, flags: {}\n",
            inode,
            handle,
            size,
            offset,
            lock_owner,
            flags
        );

        let data = self.get_data(ctx, Some(handle), inode, flags).await?;

        match data.real_handle {
            None => Err(Error::from_raw_os_error(libc::ENOENT)),
            Some(ref hd) => hd.layer.read(
                ctx,
                hd.inode,
                hd.handle.load(Ordering::Relaxed),
                w,
                size,
                offset,
                lock_owner,
                flags,
            ),
        }
    }

    fn write(
        &self,
        ctx: &Context,
        inode: Inode,
        handle: Handle,
        r: &mut dyn ZeroCopyReader,
        size: u32,
        offset: u64,
        lock_owner: Option<u64>,
        delayed_write: bool,
        flags: u32,
        fuse_flags: u32,
    ) -> Result<usize> {
        trace!(
            "WRITE: inode: {}, handle: {}, size: {}, offset: {}, lock_owner: {:?}, delayed_write: {}, flags: {}, fuse_flags: {}\n",
            inode,
            handle,
            size,
            offset,
            lock_owner,
            delayed_write,
            flags,
            fuse_flags
        );

        let data = self.get_data(ctx, Some(handle), inode, flags).await?;

        match data.real_handle {
            None => Err(Error::from_raw_os_error(libc::ENOENT)),
            Some(ref hd) => hd.layer.write(
                ctx,
                hd.inode,
                hd.handle.load(Ordering::Relaxed),
                r,
                size,
                offset,
                lock_owner,
                delayed_write,
                flags,
                fuse_flags,
            ),
        }
    }

    fn getattr(
        &self,
        ctx: &Context,
        inode: Inode,
        handle: Option<Handle>,
    ) -> Result<(stat64, Duration)> {
        trace!(
            "GETATTR: inode: {}, handle: {}\n",
            inode,
            handle.unwrap_or_default()
        );

        if !self.no_open.load(Ordering::Relaxed) {
            if let Some(h) = handle {
                if let Some(hd) = self.handles.lock().unwrap().get(&h) {
                    if let Some(ref rh) = hd.real_handle {
                        let (st, _d) = rh.layer.getattr(
                            ctx,
                            rh.inode,
                            Some(rh.handle.load(Ordering::Relaxed)),
                        )?;
                        return Ok((st, self.config.attr_timeout));
                    }
                }
            }
        }

        let node = self.lookup_node(ctx, inode, "")?;
        let (layer, _, inode) = node.first_layer_inode();
        let (st, _) = layer.getattr(ctx, inode, None)?;
        Ok((st, self.config.attr_timeout))
    }

    fn setattr(
        &self,
        ctx: &Context,
        inode: Inode,
        attr: stat64,
        handle: Option<Handle>,
        valid: SetattrValid,
    ) -> Result<(stat64, Duration)> {
        trace!("SETATTR: inode: {}\n", inode);

        // Check if upper layer exists.
        self.upper_layer
            .as_ref()
            .cloned()
            .ok_or_else(|| Error::from_raw_os_error(libc::EROFS))?;

        // deal with handle first
        if !self.no_open.load(Ordering::Relaxed) {
            if let Some(h) = handle {
                if let Some(hd) = self.handles.lock().unwrap().get(&h) {
                    if let Some(ref rhd) = hd.real_handle {
                        // handle opened in upper layer
                        if rhd.in_upper_layer {
                            let (st, _d) = rhd.layer.setattr(
                                ctx,
                                rhd.inode,
                                attr,
                                Some(rhd.handle.load(Ordering::Relaxed)),
                                valid,
                            )?;

                            return Ok((st, self.config.attr_timeout));
                        }
                    }
                }
            }
        }

        let mut node = self.lookup_node(ctx, inode, "")?;

        if !node.in_upper_layer() {
            node = self.copy_node_up(ctx, Arc::clone(&node))?
        }

        let (layer, _, real_inode) = node.first_layer_inode();
        let (st, _d) = layer.setattr(ctx, real_inode, attr, None, valid)?;
        Ok((st, self.config.attr_timeout))
    }

    fn rename(
        &self,
        _ctx: &Context,
        _olddir: Inode,
        _odlname: &CStr,
        _newdir: Inode,
        _newname: &CStr,
        _flags: u32,
    ) -> Result<()> {
        // complex, implement it later
        trace!(
            "RENAME: olddir: {}, oldname: {}, newdir: {}, newname: {}, flags: {}\n",
            _olddir,
            _odlname.to_string_lossy(),
            _newdir,
            _newname.to_string_lossy(),
            _flags
        );
        Err(Error::from_raw_os_error(libc::EXDEV))
    }

    fn mknod(
        &self,
        ctx: &Context,
        parent: Inode,
        name: &CStr,
        mode: u32,
        rdev: u32,
        umask: u32,
    ) -> Result<Entry> {
        let sname = name.to_string_lossy().to_string();
        trace!("MKNOD: parent: {}, name: {}\n", parent, sname);

        // Check if parent exists.
        let pnode = self.lookup_node(ctx, parent, "")?;
        if pnode.whiteout.load(Ordering::Relaxed) {
            return Err(Error::from_raw_os_error(libc::ENOENT));
        }

        self.do_mknod(ctx, &pnode, sname.as_str(), mode, rdev, umask)?;
        let entry = self.do_lookup(ctx, parent, sname.as_str());
        entry
    }

    fn link(&self, ctx: &Context, inode: Inode, newparent: Inode, name: &CStr) -> Result<Entry> {
        let sname = name.to_string_lossy().to_string();
        trace!(
            "LINK: inode: {}, newparent: {}, name: {}\n",
            inode,
            newparent,
            sname.as_str()
        );

        let node = self.lookup_node(ctx, inode, "")?;
        if node.whiteout.load(Ordering::Relaxed) {
            return Err(Error::from_raw_os_error(libc::ENOENT));
        }

        let newpnode = self.lookup_node(ctx, newparent, "")?;
        if newpnode.whiteout.load(Ordering::Relaxed) {
            return Err(Error::from_raw_os_error(libc::ENOENT));
        }

        self.do_link(ctx, &node, &newpnode, sname.as_str())?;
        let entry = self.do_lookup(ctx, newparent, sname.as_str());
        entry
    }

    fn symlink(&self, ctx: &Context, linkname: &CStr, parent: Inode, name: &CStr) -> Result<Entry> {
        // soft link
        let sname = name.to_string_lossy().into_owned().to_owned();
        let slinkname = linkname.to_string_lossy().into_owned().to_owned();
        trace!(
            "SYMLINK: linkname: {}, parent: {}, name: {}\n",
            linkname.to_string_lossy(),
            parent,
            sname.as_str()
        );

        let pnode = self.lookup_node(ctx, parent, "")?;
        self.do_symlink(ctx, slinkname.as_str(), &pnode, sname.as_str())?;

        let entry = self.do_lookup(ctx, parent, sname.as_str());
        entry
    }

    fn readlink(&self, ctx: &Context, inode: Inode) -> Result<Vec<u8>> {
        trace!("READLINK: inode: {}\n", inode);

        let node = self.lookup_node(ctx, inode, "")?;

        if node.whiteout.load(Ordering::Relaxed) {
            return Err(Error::from_raw_os_error(libc::ENOENT));
        }

        let (layer, _, inode) = node.first_layer_inode();
        layer.readlink(ctx, inode)
    }

    fn flush(&self, ctx: &Context, inode: Inode, handle: Handle, lock_owner: u64) -> Result<()> {
        trace!(
            "FLUSH: inode: {}, handle: {}, lock_owner: {}\n",
            inode,
            handle,
            lock_owner
        );

        if self.no_open.load(Ordering::Relaxed) {
            return Err(Error::from_raw_os_error(libc::ENOSYS));
        }

        let node = self.lookup_node(ctx, inode, "")?;

        if node.whiteout.load(Ordering::Relaxed) {
            return Err(Error::from_raw_os_error(libc::ENOENT));
        }

        let (layer, real_inode, real_handle) = self.find_real_info_from_handle(handle)?;

        // FIXME: need to test if inode matches corresponding handle?

        layer.flush(ctx, real_inode, real_handle, lock_owner)
    }

    fn fsync(&self, ctx: &Context, inode: Inode, datasync: bool, handle: Handle) -> Result<()> {
        trace!(
            "FSYNC: inode: {}, datasync: {}, handle: {}\n",
            inode,
            datasync,
            handle
        );

        self.do_fsync(ctx, inode, datasync, handle, false)
    }

    fn fsyncdir(&self, ctx: &Context, inode: Inode, datasync: bool, handle: Handle) -> Result<()> {
        trace!(
            "FSYNCDIR: inode: {}, datasync: {}, handle: {}\n",
            inode,
            datasync,
            handle
        );

        self.do_fsync(ctx, inode, datasync, handle, true)
    }

    fn access(&self, ctx: &Context, inode: Inode, mask: u32) -> Result<()> {
        trace!("ACCESS: inode: {}, mask: {}\n", inode, mask);
        let node = self.lookup_node(ctx, inode, "")?;

        if node.whiteout.load(Ordering::Relaxed) {
            return Err(Error::from_raw_os_error(libc::ENOENT));
        }

        let (layer, real_inode) = self.find_real_inode(inode)?;
        layer.access(ctx, real_inode, mask)
    }

    fn setxattr(
        &self,
        ctx: &Context,
        inode: Inode,
        name: &CStr,
        value: &[u8],
        flags: u32,
    ) -> Result<()> {
        trace!(
            "SETXATTR: inode: {}, name: {}, value: {:?}, flags: {}\n",
            inode,
            name.to_string_lossy(),
            value,
            flags
        );
        let node = self.lookup_node(ctx, inode, "")?;

        if node.whiteout.load(Ordering::Relaxed) {
            return Err(Error::from_raw_os_error(libc::ENOENT));
        }

        if !node.in_upper_layer() {
            // Copy node up.
            self.copy_node_up(ctx, Arc::clone(&node))?;
        }

        let (layer, _, real_inode) = node.first_layer_inode();

        layer.setxattr(ctx, real_inode, name, value, flags)

        // TODO: recreate node since setxattr may made dir opaque. @weizhang555.zw
    }

    fn getxattr(
        &self,
        ctx: &Context,
        inode: Inode,
        name: &CStr,
        size: u32,
    ) -> Result<GetxattrReply> {
        trace!(
            "GETXATTR: inode: {}, name: {}, size: {}\n",
            inode,
            name.to_string_lossy(),
            size
        );
        let node = self.lookup_node(ctx, inode, "")?;

        if node.whiteout.load(Ordering::Relaxed) {
            return Err(Error::from_raw_os_error(libc::ENOENT));
        }

        let (layer, real_inode) = self.find_real_inode(inode)?;

        layer.getxattr(ctx, real_inode, name, size)
    }

    fn listxattr(&self, ctx: &Context, inode: Inode, size: u32) -> Result<ListxattrReply> {
        trace!("LISTXATTR: inode: {}, size: {}\n", inode, size);
        let node = self.lookup_node(ctx, inode, "")?;

        if node.whiteout.load(Ordering::Relaxed) {
            return Err(Error::from_raw_os_error(libc::ENOENT));
        }

        let (layer, real_inode) = self.find_real_inode(inode)?;

        layer.listxattr(ctx, real_inode, size)
    }

    fn removexattr(&self, ctx: &Context, inode: Inode, name: &CStr) -> Result<()> {
        trace!(
            "REMOVEXATTR: inode: {}, name: {}\n",
            inode,
            name.to_string_lossy()
        );
        let node = self.lookup_node(ctx, inode, "")?;

        if node.whiteout.load(Ordering::Relaxed) {
            return Err(Error::from_raw_os_error(libc::ENOENT));
        }

        if !node.in_upper_layer() {
            // copy node into upper layer
            self.copy_node_up(ctx, Arc::clone(&node))?;
        }

        let (layer, _, ino) = node.first_layer_inode();
        layer.removexattr(ctx, ino, name)

        // TODO: recreate the node since removexattr may remove the opaque xattr. @weizhang555.zw
    }

    fn fallocate(
        &self,
        ctx: &Context,
        inode: Inode,
        handle: Handle,
        mode: u32,
        offset: u64,
        length: u64,
    ) -> Result<()> {
        trace!(
            "FALLOCATE: inode: {}, handle: {}, mode: {}, offset: {}, length: {}\n",
            inode,
            handle,
            mode,
            offset,
            length
        );
        // Use O_RDONLY flags which indicates no copy up.
        let data = self.get_data(ctx, Some(handle), inode, libc::O_RDONLY as u32)?;

        match data.real_handle {
            None => Err(Error::from_raw_os_error(libc::ENOENT)),
            Some(ref rhd) => {
                if !rhd.in_upper_layer {
                    // TODO: in lower layer, error out or just success?
                    return Err(Error::from_raw_os_error(libc::EROFS));
                }
                rhd.layer.fallocate(
                    ctx,
                    rhd.inode,
                    rhd.handle.load(Ordering::Relaxed),
                    mode,
                    offset,
                    length,
                )
            }
        }
    }

    fn lseek(
        &self,
        ctx: &Context,
        inode: Inode,
        handle: Handle,
        offset: u64,
        whence: u32,
    ) -> Result<u64> {
        trace!(
            "LSEEK: inode: {}, handle: {}, offset: {}, whence: {}\n",
            inode,
            handle,
            offset,
            whence
        );
        // can this be on dir? FIXME: assume file for now
        // we need special process if it can be called on dir
        let node = self.lookup_node(ctx, inode, "")?;

        if node.whiteout.load(Ordering::Relaxed) {
            return Err(Error::from_raw_os_error(libc::ENOENT));
        }

        let st = node.stat64(ctx)?;
        if utils::is_dir(st) {
            error!("lseek on directory");
            return Err(Error::from_raw_os_error(libc::EINVAL));
        }

        let (layer, real_inode, real_handle) = self.find_real_info_from_handle(handle)?;
        layer.lseek(ctx, real_inode, real_handle, offset, whence)
    }
}
