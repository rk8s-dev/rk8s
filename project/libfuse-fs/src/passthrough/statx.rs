// Copyright 2021 Red Hat, Inc. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

use std::ffi::CStr;
use std::io;
use std::mem::MaybeUninit;
use std::os::unix::io::AsRawFd;

#[cfg(target_os = "linux")]
use libc::{STATX_BTIME, statx_timestamp};
#[cfg(target_os = "macos")]
#[allow(non_camel_case_types)]
#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub struct statx_timestamp {
    pub tv_sec: i64,
    pub tv_nsec: u32,
    pub __reserved: i32,
}
#[cfg(target_os = "macos")]
#[allow(dead_code)]
pub const STATX_BTIME: u32 = 0x800;

#[allow(unused_imports)]
use super::{
    EMPTY_CSTR,
    os_compat::{STATX_BASIC_STATS, STATX_MNT_ID, statx_st},
};
#[cfg(target_os = "linux")]
use crate::passthrough::file_handle::FileHandle;

pub type MountId = u64;

pub struct StatExt {
    #[cfg(target_os = "linux")]
    pub st: libc::stat64,
    #[cfg(target_os = "macos")]
    pub st: libc::stat,
    pub mnt_id: MountId,
    // Using Option<> for easier testing.
    pub btime: Option<statx_timestamp>,
}

/*
 * Fields in libc::statx are only valid if their respective flag in
 * .stx_mask is set.  This trait provides functions that allow safe
 * access to the libc::statx components we are interested in.
 *
 * (The implementations of these functions need to check whether the
 * associated flag is set, and then extract the respective information
 * to return it.)
 */
#[allow(dead_code)]
trait SafeStatXAccess {
    #[cfg(target_os = "linux")]
    fn stat64(&self) -> Option<libc::stat64>;
    fn mount_id(&self) -> Option<MountId>;
}

#[cfg(target_os = "linux")]
impl SafeStatXAccess for statx_st {
    fn stat64(&self) -> Option<libc::stat64> {
        fn makedev(maj: libc::c_uint, min: libc::c_uint) -> libc::dev_t {
            libc::makedev(maj as libc::c_uint, min as libc::c_uint)
        }

        if self.stx_mask & STATX_BASIC_STATS != 0 {
            /*
             * Unfortunately, we cannot use an initializer to create the
             * stat64 object, because it may contain padding and reserved
             * fields (depending on the architecture), and it does not
             * implement the Default trait.
             * So we take a zeroed struct and set what we can.
             * (Zero in all fields is wrong, but safe.)
             */
            let mut st = unsafe { MaybeUninit::<libc::stat64>::zeroed().assume_init() };

            st.st_dev = makedev(self.stx_dev_major, self.stx_dev_minor);
            st.st_ino = self.stx_ino;
            st.st_mode = self.stx_mode as _;
            st.st_nlink = self.stx_nlink as _;
            st.st_uid = self.stx_uid;
            st.st_gid = self.stx_gid;
            st.st_rdev = makedev(self.stx_rdev_major, self.stx_rdev_minor);
            st.st_size = self.stx_size as _;
            st.st_blksize = self.stx_blksize as _;
            st.st_blocks = self.stx_blocks as _;
            st.st_atime = self.stx_atime.tv_sec;
            st.st_atime_nsec = self.stx_atime.tv_nsec as _;
            st.st_mtime = self.stx_mtime.tv_sec;
            st.st_mtime_nsec = self.stx_mtime.tv_nsec as _;
            st.st_ctime = self.stx_ctime.tv_sec;
            st.st_ctime_nsec = self.stx_ctime.tv_nsec as _;

            Some(st)
        } else {
            None
        }
    }

    fn mount_id(&self) -> Option<MountId> {
        if self.stx_mask & STATX_MNT_ID != 0 {
            Some(self.stx_mnt_id)
        } else {
            None
        }
    }
}

#[cfg(target_os = "linux")]
fn get_mount_id(dir: &impl AsRawFd, path: &CStr) -> Option<MountId> {
    match FileHandle::from_name_at(dir, path) {
        Ok(Some(v)) => Some(v.mnt_id),
        _ => None,
    }
}

// Only works on Linux, and libc::SYS_statx is only defined for these
// environments
/// Performs a statx() syscall.  libc provides libc::statx() that does
/// the same, however, the system's libc may not have a statx() wrapper
/// (e.g. glibc before 2.28), so linking to it may fail.
/// libc::syscall() and libc::SYS_statx are always present, though, so
/// we can safely rely on them.
#[cfg(target_os = "linux")]
fn do_statx(
    dirfd: libc::c_int,
    pathname: *const libc::c_char,
    flags: libc::c_int,
    mask: libc::c_uint,
    statxbuf: *mut statx_st,
) -> libc::c_int {
    (unsafe { libc::syscall(libc::SYS_statx, dirfd, pathname, flags, mask, statxbuf) })
        as libc::c_int
}

/// Execute `statx()` to get extended status with mount id.
pub fn statx(dir: &impl AsRawFd, path: Option<&CStr>) -> io::Result<StatExt> {
    #[allow(unused)]
    let mut stx_ui = MaybeUninit::<statx_st>::zeroed();

    // Linux implementation
    #[cfg(target_os = "linux")]
    {
        // ... (existing Linux code) ...
        // Safe because this is a constant value and a valid C string.
        let path =
            path.unwrap_or_else(|| unsafe { CStr::from_bytes_with_nul_unchecked(EMPTY_CSTR) });

        // Safe because the kernel will only write data in `stx_ui` and we
        // check the return value.
        let res = do_statx(
            dir.as_raw_fd(),
            path.as_ptr(),
            libc::AT_EMPTY_PATH | libc::AT_SYMLINK_NOFOLLOW,
            STATX_BASIC_STATS | STATX_MNT_ID | STATX_BTIME,
            stx_ui.as_mut_ptr(),
        );
        if res >= 0 {
            // Safe because we are only going to use the SafeStatXAccess
            // trait methods
            let stx = unsafe { stx_ui.assume_init() };

            // if `statx()` doesn't provide the mount id (before kernel 5.8),
            // let's try `name_to_handle_at()`, if everything fails just use 0
            let mnt_id = stx
                .mount_id()
                .or_else(|| get_mount_id(dir, path))
                .unwrap_or(0);
            let st = stx
                .stat64()
                .ok_or_else(|| io::Error::from_raw_os_error(libc::ENOSYS))?;
            let btime = Some(stx.stx_btime);
            Ok(StatExt { st, mnt_id, btime })
        } else {
            Err(io::Error::last_os_error())
        }
    }
    #[cfg(target_os = "macos")]
    {
        // use std::os::unix::ffi::OsStrExt;
        let path_cstr =
            path.unwrap_or_else(|| unsafe { CStr::from_bytes_with_nul_unchecked(EMPTY_CSTR) });

        #[cfg(target_os = "linux")]
        let mut st = MaybeUninit::<libc::stat64>::zeroed();
        #[cfg(target_os = "macos")]
        let mut st = MaybeUninit::<libc::stat>::zeroed();

        let bytes = path_cstr.to_bytes();
        let res = if bytes.is_empty() {
            unsafe { libc::fstat(dir.as_raw_fd(), st.as_mut_ptr()) }
        } else {
            unsafe {
                libc::fstatat(
                    dir.as_raw_fd(),
                    path_cstr.as_ptr(),
                    st.as_mut_ptr(),
                    libc::AT_SYMLINK_NOFOLLOW,
                )
            }
        };
        if res == 0 {
            let st = unsafe { st.assume_init() };
            let mnt_id = 0; // Dummy mount id
            // btime on macos is st_birthtimespec, but referencing it fails for some reason.
            // We'll trust the error and just use st_mtimespec as fallback or 0.
            let btime = statx_timestamp {
                tv_sec: st.st_mtime,
                tv_nsec: st.st_mtime_nsec as u32,
                #[cfg(target_os = "macos")]
                __reserved: 0,
            };
            Ok(StatExt {
                st,
                mnt_id,
                btime: Some(btime),
            })
        } else {
            Err(io::Error::last_os_error())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;
    use std::fs::File;

    #[test]
    fn test_statx() {
        let topdir = std::env::current_dir().unwrap();
        let dir = File::open(&topdir).unwrap();
        let filename = CString::new("Cargo.toml").unwrap();

        let _st1 = statx(&dir, None).unwrap();
        let _st2 = statx(&dir, Some(&filename)).unwrap();
        #[cfg(target_os = "linux")]
        {
            let mnt_id = get_mount_id(&dir, &filename).unwrap();
            assert_eq!(_st1.mnt_id, _st2.mnt_id);
            assert_eq!(_st1.mnt_id, mnt_id);
        }
    }
}
