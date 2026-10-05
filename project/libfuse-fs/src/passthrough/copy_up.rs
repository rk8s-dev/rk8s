//! Descriptor-bound private copy-up storage, outside the upper namespace.

use super::PassthroughFs;
use crate::context::OperationContext;
use crate::unionfs::copy_up::CopyUpFile;
use async_trait::async_trait;
use std::ffi::{CString, OsStr};
use std::fs::File;
use std::io::{Error, Result};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::{
    ffi::OsStrExt,
    fs::{FileExt, MetadataExt},
};

const PAYLOAD: &std::ffi::CStr = c"payload";

struct StageDirectory {
    base: File,
    name: CString,
    directory: File,
}

impl Drop for StageDirectory {
    fn drop(&mut self) {
        // These are fixed names beneath owned directory descriptors; no path
        // traversal or recursive deletion is involved.
        unsafe {
            libc::unlinkat(self.directory.as_raw_fd(), PAYLOAD.as_ptr(), 0);
        }
        if directory_matches(&self.base, &self.name, &self.directory).unwrap_or(false) {
            unsafe {
                libc::unlinkat(
                    self.base.as_raw_fd(),
                    self.name.as_ptr(),
                    libc::AT_REMOVEDIR,
                );
            }
        }
    }
}

pub(crate) struct PrivateCopyUp {
    stage: StageDirectory,
    file: File,
    parent: File,
    final_name: Option<CString>,
    committed: bool,
    uid: u32,
    gid: u32,
}

fn open_at(parent: &File, name: &std::ffi::CStr, flags: i32, mode: u32) -> Result<File> {
    let fd = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name.as_ptr(),
            flags,
            mode as libc::mode_t,
        )
    };
    if fd < 0 {
        return Err(Error::last_os_error());
    }
    Ok(unsafe { File::from_raw_fd(fd) })
}

fn directory_matches(parent: &File, name: &std::ffi::CStr, file: &File) -> Result<bool> {
    let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
    if unsafe {
        libc::fstatat(
            parent.as_raw_fd(),
            name.as_ptr(),
            stat.as_mut_ptr(),
            libc::AT_SYMLINK_NOFOLLOW,
        )
    } != 0
    {
        return Err(Error::last_os_error());
    }
    let stat = unsafe { stat.assume_init() };
    let owned = file.metadata()?;
    Ok(stat.st_dev as u64 == owned.dev() && stat.st_ino as u64 == owned.ino())
}

impl PassthroughFs {
    pub(crate) async fn begin_private_copy_up(
        &self,
        ctx: OperationContext,
        parent: u64,
        mode: u32,
    ) -> Result<PrivateCopyUp> {
        let inode = self.inode_map.get(parent).await?;
        let parent = inode.get_file()?;
        let root = self.cfg.root_dir.canonicalize()?;
        let base_path = match &self.cfg.copy_up_work_dir {
            Some(path) => path.canonicalize()?,
            None => root
                .parent()
                .ok_or_else(|| Error::from_raw_os_error(libc::EINVAL))?
                .to_path_buf(),
        };
        if base_path.starts_with(&root) {
            return Err(Error::from_raw_os_error(libc::EINVAL));
        }
        let base_path = CString::new(base_path.as_os_str().as_bytes())
            .map_err(|_| Error::from_raw_os_error(libc::EINVAL))?;
        let fd = unsafe {
            libc::open(
                base_path.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            return Err(Error::last_os_error());
        }
        let base = unsafe { File::from_raw_fd(fd) };
        if base.metadata()?.dev() != parent.metadata()?.dev() {
            return Err(Error::from_raw_os_error(libc::EXDEV));
        }
        let name = CString::new(format!(".libfuse-copyup-{}", uuid::Uuid::new_v4()))
            .expect("uuid component");
        if unsafe { libc::mkdirat(base.as_raw_fd(), name.as_ptr(), 0o700) } != 0 {
            return Err(Error::last_os_error());
        }
        let directory = match open_at(
            &base,
            &name,
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            0,
        ) {
            Ok(directory) => directory,
            Err(error) => {
                unsafe {
                    libc::unlinkat(base.as_raw_fd(), name.as_ptr(), libc::AT_REMOVEDIR);
                }
                return Err(error);
            }
        };
        let stage = StageDirectory {
            base,
            name,
            directory,
        };
        let file = open_at(
            &stage.directory,
            PAYLOAD,
            libc::O_RDWR | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            0o600,
        )?;
        let uid = ctx.uid.unwrap_or(self.cfg.mapping.get_uid(ctx.req.uid));
        let gid = ctx.gid.unwrap_or(self.cfg.mapping.get_gid(ctx.req.gid));
        let stat = file.metadata()?;
        if (stat.uid() != uid || stat.gid() != gid)
            && unsafe { libc::fchown(file.as_raw_fd(), uid, gid) } != 0
        {
            return Err(Error::last_os_error());
        }
        if unsafe { libc::fchmod(file.as_raw_fd(), (mode & 0o7777) as libc::mode_t) } != 0 {
            return Err(Error::last_os_error());
        }
        let owner = stage.directory.metadata()?;
        if (owner.uid() != uid || owner.gid() != gid)
            && unsafe { libc::fchown(stage.directory.as_raw_fd(), uid, gid) } != 0
        {
            return Err(Error::last_os_error());
        }
        Ok(PrivateCopyUp {
            stage,
            file,
            parent,
            final_name: None,
            committed: false,
            uid,
            gid,
        })
    }
}

#[async_trait]
impl CopyUpFile for PrivateCopyUp {
    async fn write(&mut self, offset: u64, data: &[u8]) -> Result<u32> {
        let written = self.file.write_at(data, offset)?;
        written
            .try_into()
            .map_err(|_| Error::from_raw_os_error(libc::EOVERFLOW))
    }

    fn promote(&mut self, name: &OsStr) -> Result<()> {
        let bytes = name.as_bytes();
        if bytes.is_empty() || bytes == b"." || bytes == b".." || bytes.contains(&b'/') {
            return Err(Error::from_raw_os_error(libc::EINVAL));
        }
        let name = CString::new(bytes).map_err(|_| Error::from_raw_os_error(libc::EINVAL))?;
        let _credentials = super::util::set_creds(self.uid, self.gid)?;
        // A same-device hard link creates the complete final inode atomically
        // and refuses to replace any concurrently created upper name.
        if unsafe {
            libc::linkat(
                self.stage.directory.as_raw_fd(),
                PAYLOAD.as_ptr(),
                self.parent.as_raw_fd(),
                name.as_ptr(),
                0,
            )
        } != 0
        {
            return Err(Error::last_os_error());
        }
        self.final_name = Some(name);
        if unsafe { libc::unlinkat(self.stage.directory.as_raw_fd(), PAYLOAD.as_ptr(), 0) } != 0 {
            return Err(Error::last_os_error());
        }
        Ok(())
    }

    fn verify_promotion(&self) -> Result<()> {
        let name = self
            .final_name
            .as_ref()
            .ok_or_else(|| Error::from_raw_os_error(libc::EINVAL))?;
        if directory_matches(&self.parent, name, &self.file)? {
            Ok(())
        } else {
            Err(Error::from_raw_os_error(libc::ESTALE))
        }
    }

    fn commit(&mut self) {
        self.committed = true;
    }
}

impl Drop for PrivateCopyUp {
    fn drop(&mut self) {
        if !self.committed {
            if let Some(name) = &self.final_name {
                if directory_matches(&self.parent, name, &self.file).unwrap_or(false) {
                    unsafe {
                        libc::unlinkat(self.parent.as_raw_fd(), name.as_ptr(), 0);
                    }
                }
            }
        }
        // File and directory owners close their actual native descriptors.
    }
}
