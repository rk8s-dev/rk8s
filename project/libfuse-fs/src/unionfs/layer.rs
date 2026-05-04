#![allow(clippy::unnecessary_cast)]
use async_trait::async_trait;
use rfuse3::raw::reply::{FileAttr, ReplyCreated, ReplyXAttr};
use rfuse3::raw::{ObjectSafeFilesystem, Request, reply::ReplyEntry};
use rfuse3::{Inode, Result};
use std::ffi::OsStr;
use std::io::Error;
use std::time::Duration;

use crate::context::OperationContext;
use crate::passthrough::PassthroughFs;
use crate::util::whiteout::{OCI_OPAQUE_MARKER, WhiteoutFormat, oci_whiteout_name};
pub const OPAQUE_XATTR_LEN: u32 = 16;
pub const OPAQUE_XATTR: &str = "user.fuseoverlayfs.opaque";
pub const UNPRIVILEGED_OPAQUE_XATTR: &str = "user.overlay.opaque";
pub const PRIVILEGED_OPAQUE_XATTR: &str = "trusted.overlay.opaque";

#[cfg(target_os = "macos")]
type Stat64 = libc::stat;
#[cfg(target_os = "linux")]
type Stat64 = libc::stat64;

/// A filesystem must implement Layer trait, or it cannot be used as an OverlayFS layer.
#[async_trait]
pub trait Layer: ObjectSafeFilesystem {
    /// Return the root inode number
    fn root_inode(&self) -> Inode;

    /// Whiteout format used by this layer. Default is `CharDev` on Linux and
    /// `OciWhiteout` on macOS; backends may override via config.
    fn whiteout_format(&self) -> WhiteoutFormat {
        WhiteoutFormat::default()
    }

    /// Resolve `inode` to the absolute host filesystem path that backs it,
    /// if such a mapping exists. Returns `None` for layers that lack a 1:1
    /// host-fs mapping. See `overlayfs::Layer::host_path_of` for details.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    async fn host_path_of(&self, _inode: Inode) -> Option<std::path::PathBuf> {
        None
    }
    /// Create whiteout file with name <name>.
    ///
    /// If this call is successful then the lookup count of the `Inode` associated with the returned
    /// `Entry` must be increased by 1.
    async fn create_whiteout(
        &self,
        ctx: Request,
        parent: Inode,
        name: &OsStr,
    ) -> Result<ReplyEntry> {
        let ino: u64 = parent;
        match self.whiteout_format() {
            WhiteoutFormat::CharDev => {
                match self.lookup(ctx, ino, name).await {
                    Ok(v) => {
                        if is_whiteout(&v.attr) {
                            return Ok(v);
                        }
                        if v.attr.ino != 0 {
                            self.forget(ctx, v.attr.ino, 1).await;
                            return Err(Error::from_raw_os_error(libc::EEXIST).into());
                        }
                    }
                    Err(e) => {
                        let e: std::io::Error = e.into();
                        match e.raw_os_error() {
                            Some(raw_error) => {
                                if raw_error != libc::ENOENT {
                                    return Err(e.into());
                                }
                            }
                            None => return Err(e.into()),
                        }
                    }
                }
                let dev = libc::makedev(0, 0);
                let mode = (libc::S_IFCHR as u32) | 0o777;
                self.mknod(ctx, ino, name, mode, dev as u32).await
            }
            WhiteoutFormat::OciWhiteout => {
                oci_create_marker(self, ctx, ino, &oci_whiteout_name(name)).await
            }
        }
    }

    /// Delete whiteout file with name <name>.
    async fn delete_whiteout(&self, ctx: Request, parent: Inode, name: &OsStr) -> Result<()> {
        let ino: u64 = parent;
        match self.whiteout_format() {
            WhiteoutFormat::CharDev => {
                match self.lookup(ctx, ino, name).await {
                    Ok(v) => {
                        if v.attr.ino != 0 {
                            self.forget(ctx, v.attr.ino, 1).await;
                        }
                        if is_whiteout(&v.attr) {
                            return match self.unlink(ctx, ino, name).await {
                                Ok(()) => Ok(()),
                                Err(e) => {
                                    let ie: std::io::Error = e.into();
                                    if ie.raw_os_error() == Some(libc::ENOENT) {
                                        Ok(())
                                    } else {
                                        Err(ie.into())
                                    }
                                }
                            };
                        }
                        if v.attr.ino != 0 {
                            return Err(Error::from_raw_os_error(libc::EINVAL).into());
                        }
                    }
                    Err(e) => {
                        let ie: std::io::Error = e.into();
                        if ie.raw_os_error() != Some(libc::ENOENT) {
                            return Err(ie.into());
                        }
                    }
                }
                Ok(())
            }
            WhiteoutFormat::OciWhiteout => {
                let wh = oci_whiteout_name(name);
                match self.unlink(ctx, ino, &wh).await {
                    Ok(()) => Ok(()),
                    Err(e) => {
                        let ie: std::io::Error = e.into();
                        if ie.raw_os_error() == Some(libc::ENOENT) {
                            Ok(())
                        } else {
                            Err(ie.into())
                        }
                    }
                }
            }
        }
    }

    /// Check if the Inode is a whiteout file.
    ///
    /// **Note**: this overload is `CharDev`-only by design — see the trait
    /// definition for explanation. In `OciWhiteout` mode this returns
    /// `Ok(false)`.
    async fn is_whiteout(&self, ctx: Request, inode: Inode) -> Result<bool> {
        match self.whiteout_format() {
            WhiteoutFormat::CharDev => {
                let rep = self.getattr(ctx, inode, None, 0).await?;
                Ok(is_whiteout(&rep.attr))
            }
            WhiteoutFormat::OciWhiteout => Ok(false),
        }
    }

    /// Set the directory to opaque.
    async fn set_opaque(&self, ctx: Request, inode: Inode) -> Result<()> {
        let ino: u64 = inode;

        let rep = self.getattr(ctx, ino, None, 0).await?;
        if !is_dir(&rep.attr) {
            return Err(Error::from_raw_os_error(libc::ENOTDIR).into());
        }

        match self.whiteout_format() {
            WhiteoutFormat::CharDev => {
                self.setxattr(ctx, ino, OsStr::new(OPAQUE_XATTR), b"y", 0, 0)
                    .await
            }
            WhiteoutFormat::OciWhiteout => {
                oci_create_marker(self, ctx, ino, OsStr::new(OCI_OPAQUE_MARKER))
                    .await
                    .map(|_| ())
            }
        }
    }

    /// Check if the directory is opaque.
    async fn is_opaque(&self, ctx: Request, inode: Inode) -> Result<bool> {
        let ino: u64 = inode;

        let attr: rfuse3::raw::prelude::ReplyAttr = self.getattr(ctx, ino, None, 0).await?;
        if !is_dir(&attr.attr) {
            return Err(Error::from_raw_os_error(libc::ENOTDIR).into());
        }

        if matches!(self.whiteout_format(), WhiteoutFormat::OciWhiteout) {
            let marker = OsStr::new(OCI_OPAQUE_MARKER);
            return match self.lookup(ctx, ino, marker).await {
                Ok(v) => {
                    if v.attr.ino == 0 {
                        Ok(false)
                    } else {
                        self.forget(ctx, v.attr.ino, 1).await;
                        Ok(true)
                    }
                }
                Err(e) => {
                    let ie: std::io::Error = e.into();
                    if ie.raw_os_error() == Some(libc::ENOENT) {
                        Ok(false)
                    } else {
                        Err(ie.into())
                    }
                }
            };
        }

        let check_attr = |inode: Inode, attr_name: &'static str, attr_size: u32| async move {
            let cname = OsStr::new(attr_name);
            match self.getxattr(ctx, inode, cname, attr_size).await {
                Ok(v) => {
                    // xattr name exists and we get value.
                    if let ReplyXAttr::Data(bufs) = v
                        && bufs.len() == 1
                        && bufs[0].eq_ignore_ascii_case(&b'y')
                    {
                        return Ok(true);
                    }
                    // No value found, go on to next check.
                    Ok(false)
                }
                Err(e) => {
                    let ioerror: std::io::Error = e.into();
                    if ioerror.raw_os_error() == Some(libc::ENODATA) {
                        return Ok(false);
                    }
                    #[cfg(target_os = "macos")]
                    if ioerror.raw_os_error() == Some(libc::ENOATTR) {
                        return Ok(false);
                    }

                    Err(e)
                }
            }
        };

        // A directory is made opaque by setting some specific xattr to "y".
        // See ref: https://docs.kernel.org/filesystems/overlayfs.html#whiteouts-and-opaque-directories

        // Check our customized version of the xattr "user.fuseoverlayfs.opaque".
        let is_opaque = check_attr(ino, OPAQUE_XATTR, OPAQUE_XATTR_LEN).await?;
        if is_opaque {
            return Ok(true);
        }

        // Also check for the unprivileged version of the xattr "trusted.overlay.opaque".
        let is_opaque = check_attr(ino, PRIVILEGED_OPAQUE_XATTR, OPAQUE_XATTR_LEN).await?;
        if is_opaque {
            return Ok(true);
        }

        // Also check for the unprivileged version of the xattr "user.overlay.opaque".
        let is_opaque = check_attr(ino, UNPRIVILEGED_OPAQUE_XATTR, OPAQUE_XATTR_LEN).await?;
        if is_opaque {
            return Ok(true);
        }

        Ok(false)
    }

    /// Create a file with explicit ownership context.
    /// Uses OperationContext to allow overriding UID/GID for internal operations like copy-up.
    async fn create_with_context(
        &self,
        _ctx: OperationContext,
        _parent: Inode,
        _name: &OsStr,
        _mode: u32,
        _flags: u32,
    ) -> Result<ReplyCreated> {
        Err(Error::from_raw_os_error(libc::ENOSYS).into())
    }

    /// Create a directory with explicit ownership context.
    /// Uses OperationContext to allow overriding UID/GID for internal operations like copy-up.
    async fn mkdir_with_context(
        &self,
        _ctx: OperationContext,
        _parent: Inode,
        _name: &OsStr,
        _mode: u32,
        _umask: u32,
    ) -> Result<ReplyEntry> {
        Err(Error::from_raw_os_error(libc::ENOSYS).into())
    }

    /// Create a symbolic link with explicit ownership context.
    /// Uses OperationContext to allow overriding UID/GID for internal operations like copy-up.
    async fn symlink_with_context(
        &self,
        _ctx: OperationContext,
        _parent: Inode,
        _name: &OsStr,
        _link: &OsStr,
    ) -> Result<ReplyEntry> {
        Err(Error::from_raw_os_error(libc::ENOSYS).into())
    }

    /// Retrieve metadata with optional ID mapping control.
    ///
    /// - `mapping: true`: Returns attributes as seen inside the container (mapped).
    /// - `mapping: false`: Returns raw attributes on the host filesystem (unmapped).
    async fn getattr_with_mapping(
        &self,
        _inode: Inode,
        _handle: Option<u64>,
        _mapping: bool,
    ) -> std::io::Result<(Stat64, Duration)> {
        Err(std::io::Error::from_raw_os_error(libc::ENOSYS))
    }
}

#[async_trait]
impl Layer for PassthroughFs {
    fn root_inode(&self) -> Inode {
        1
    }

    fn whiteout_format(&self) -> WhiteoutFormat {
        self.config().whiteout_format
    }

    async fn host_path_of(&self, inode: Inode) -> Option<std::path::PathBuf> {
        self.passthrough_host_path(inode).await
    }

    async fn create_with_context(
        &self,
        ctx: OperationContext,
        parent: Inode,
        name: &OsStr,
        mode: u32,
        flags: u32,
    ) -> Result<ReplyCreated> {
        PassthroughFs::do_create_helper(
            self,
            ctx.req,
            parent,
            name,
            mode,
            flags,
            ctx.uid.unwrap_or(ctx.req.uid),
            ctx.gid.unwrap_or(ctx.req.gid),
        )
        .await
    }

    async fn mkdir_with_context(
        &self,
        ctx: OperationContext,
        parent: Inode,
        name: &OsStr,
        mode: u32,
        umask: u32,
    ) -> Result<ReplyEntry> {
        PassthroughFs::do_mkdir_helper(
            self,
            ctx.req,
            parent,
            name,
            mode,
            umask,
            ctx.uid.unwrap_or(ctx.req.uid),
            ctx.gid.unwrap_or(ctx.req.gid),
        )
        .await
    }

    async fn symlink_with_context(
        &self,
        ctx: OperationContext,
        parent: Inode,
        name: &OsStr,
        link: &OsStr,
    ) -> Result<ReplyEntry> {
        PassthroughFs::do_symlink_helper(
            self,
            ctx.req,
            parent,
            name,
            link,
            ctx.uid.unwrap_or(ctx.req.uid),
            ctx.gid.unwrap_or(ctx.req.gid),
        )
        .await
    }

    async fn getattr_with_mapping(
        &self,
        inode: Inode,
        handle: Option<u64>,
        mapping: bool,
    ) -> std::io::Result<(Stat64, Duration)> {
        PassthroughFs::do_getattr_inner(self, inode, handle, mapping).await
    }
}
pub(crate) fn is_dir(st: &FileAttr) -> bool {
    st.kind.const_into_mode_t() & libc::S_IFMT == libc::S_IFDIR
}

pub(crate) fn is_chardev(st: &FileAttr) -> bool {
    st.kind.const_into_mode_t() & libc::S_IFMT == libc::S_IFCHR
}

pub(crate) fn is_whiteout(st: &FileAttr) -> bool {
    // A whiteout is created as a character device with 0/0 device number.
    // See ref: https://docs.kernel.org/filesystems/overlayfs.html#whiteouts-and-opaque-directories
    let major = libc::major(st.rdev as libc::dev_t);
    let minor = libc::minor(st.rdev as libc::dev_t);
    is_chardev(st) && major == 0 && minor == 0
}

/// Create an OCI whiteout / opaque-marker file (regular file, mode 0).
///
/// Uses `ObjectSafeFilesystem::create` rather than `mknod(S_IFREG, ..)`
/// because Darwin's `mknod(2)` rejects regular-file modes with EINVAL. The fh
/// from `create` is released immediately — callers only want the entry attrs.
async fn oci_create_marker<F: ObjectSafeFilesystem + ?Sized>(
    fs: &F,
    ctx: Request,
    parent: Inode,
    marker: &OsStr,
) -> Result<ReplyEntry> {
    match fs.lookup(ctx, parent, marker).await {
        Ok(v) if v.attr.ino != 0 => return Ok(v),
        Ok(_) => {}
        Err(e) => {
            let ie: std::io::Error = e.into();
            if ie.raw_os_error() != Some(libc::ENOENT) {
                return Err(ie.into());
            }
        }
    }
    let flags = (libc::O_CREAT | libc::O_EXCL | libc::O_WRONLY) as u32;
    let created = fs.create(ctx, parent, marker, 0o000, flags).await?;
    let _ = fs
        .release(ctx, created.attr.ino, created.fh, flags, 0, false)
        .await;
    Ok(ReplyEntry {
        ttl: created.ttl,
        attr: created.attr,
        generation: created.generation,
    })
}

#[cfg(test)]
mod test {
    use std::{ffi::OsStr, path::PathBuf};

    use rfuse3::raw::{Filesystem as _, Request};

    use crate::{
        passthrough::{PassthroughArgs, PassthroughFs, config::Config, new_passthroughfs_layer},
        unionfs::layer::Layer,
        unwrap_or_skip_eperm,
        util::whiteout::WhiteoutFormat,
    };

    #[tokio::test]
    async fn delete_missing_oci_whiteout_is_idempotent() {
        let temp_dir = tempfile::tempdir().unwrap();
        let fs = PassthroughFs::<()>::new(Config {
            root_dir: temp_dir.path().to_path_buf(),
            do_import: true,
            whiteout_format: WhiteoutFormat::OciWhiteout,
            ..Default::default()
        })
        .unwrap();
        unwrap_or_skip_eperm!(fs.init(Request::default()).await, "fs init");
        unwrap_or_skip_eperm!(
            fs.delete_whiteout(Request::default(), 1, OsStr::new("missing"))
                .await,
            "delete_whiteout missing OCI marker"
        );
    }

    // Mark as ignored by default; run with: RUN_PRIVILEGED_TESTS=1 cargo test -- --ignored
    #[ignore]
    #[tokio::test]
    async fn test_whiteout_create_delete() {
        let temp_dir = "/tmp/test_whiteout/t2";
        let rootdir = PathBuf::from(temp_dir);
        std::fs::create_dir_all(&rootdir).unwrap();
        if std::env::var("RUN_PRIVILEGED_TESTS").ok().as_deref() != Some("1") {
            eprintln!("skip test_whiteout_create_delete: RUN_PRIVILEGED_TESTS!=1");
            return;
        }
        let fs = unwrap_or_skip_eperm!(
            new_passthroughfs_layer(PassthroughArgs {
                root_dir: rootdir,
                mapping: None::<&str>
            })
            .await,
            "init passthrough layer"
        );
        let _ = unwrap_or_skip_eperm!(fs.init(Request::default()).await, "fs init");
        let white_name = OsStr::new(&"test");
        let res = unwrap_or_skip_eperm!(
            fs.create_whiteout(Request::default(), 1, white_name).await,
            "create whiteout"
        );

        print!("{res:?}");
        let res = fs.delete_whiteout(Request::default(), 1, white_name).await;
        if res.is_err() {
            panic!("{res:?}");
        }
        let _ = fs.destroy(Request::default()).await;
    }

    #[tokio::test]
    async fn test_is_opaque_on_non_directory() {
        let temp_dir = "/tmp/test_opaque_non_dir/t2";
        let rootdir = PathBuf::from(temp_dir);
        std::fs::create_dir_all(&rootdir).unwrap();
        if std::env::var("RUN_PRIVILEGED_TESTS").ok().as_deref() != Some("1") {
            eprintln!("skip test_is_opaque_on_non_directory: RUN_PRIVILEGED_TESTS!=1");
            return;
        }
        let fs = unwrap_or_skip_eperm!(
            new_passthroughfs_layer(PassthroughArgs {
                root_dir: rootdir,
                mapping: None::<&str>
            })
            .await,
            "init passthrough layer"
        );
        let _ = unwrap_or_skip_eperm!(fs.init(Request::default()).await, "fs init");

        // Create a file
        let file_name = OsStr::new("not_a_dir");
        let _ = unwrap_or_skip_eperm!(
            fs.create(Request::default(), 1, file_name, 0o644, 0).await,
            "create file"
        );

        // Lookup to get the inode of the file
        let entry = unwrap_or_skip_eperm!(
            fs.lookup(Request::default(), 1, file_name).await,
            "lookup file"
        );
        let file_inode = entry.attr.ino;

        // is_opaque should return ENOTDIR error
        let res = fs.is_opaque(Request::default(), file_inode).await;
        assert!(res.is_err());
        let err = res.err().unwrap();
        let ioerr: std::io::Error = err.into();
        assert_eq!(ioerr.raw_os_error(), Some(libc::ENOTDIR));

        // Clean up
        let _ = fs.unlink(Request::default(), 1, file_name).await;
        let _ = fs.destroy(Request::default()).await;
    }

    #[tokio::test]
    async fn test_set_opaque_on_non_directory() {
        let temp_dir = "/tmp/test_set_opaque_non_dir/t2";
        let rootdir = PathBuf::from(temp_dir);
        std::fs::create_dir_all(&rootdir).unwrap();
        if std::env::var("RUN_PRIVILEGED_TESTS").ok().as_deref() != Some("1") {
            eprintln!("skip test_set_opaque_on_non_directory: RUN_PRIVILEGED_TESTS!=1");
            return;
        }
        let fs = unwrap_or_skip_eperm!(
            new_passthroughfs_layer(PassthroughArgs {
                root_dir: rootdir,
                mapping: None::<&str>
            })
            .await,
            "init passthrough layer"
        );
        let _ = unwrap_or_skip_eperm!(fs.init(Request::default()).await, "fs init");

        // Create a file
        let file_name = OsStr::new("not_a_dir2");
        let _ = unwrap_or_skip_eperm!(
            fs.create(Request::default(), 1, file_name, 0o644, 0).await,
            "create file"
        );

        // Lookup to get the inode of the file
        let entry = unwrap_or_skip_eperm!(
            fs.lookup(Request::default(), 1, file_name).await,
            "lookup file"
        );
        let file_inode = entry.attr.ino;

        // set_opaque should return ENOTDIR error
        let res = fs.set_opaque(Request::default(), file_inode).await;
        assert!(res.is_err());
        let err = res.err().unwrap();
        let ioerr: std::io::Error = err.into();
        assert_eq!(ioerr.raw_os_error(), Some(libc::ENOTDIR));

        // Clean up
        let _ = fs.unlink(Request::default(), 1, file_name).await;
        let _ = fs.destroy(Request::default()).await;
    }
}
