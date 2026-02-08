// Copyright (C) 2023 Alibaba Cloud. All rights reserved.
// Copyright 2021 Red Hat, Inc. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE-BSD-3-Clause file.

use std::cmp::Ordering;
use std::ffi::CStr;
use std::fmt::{Debug, Formatter};
use std::fs::File;
use std::io;
use std::os::fd::AsFd;
#[cfg(target_os = "linux")]
use std::os::unix::io::FromRawFd;
use std::os::unix::io::{AsRawFd, RawFd};
use std::sync::Arc;

#[allow(unused_imports)]
use tracing::error;
use vmm_sys_util::fam::{FamStruct, FamStructWrapper};

use super::EMPTY_CSTR;
#[cfg(target_os = "linux")]
use super::mount_fd::MountId;
use super::mount_fd::{MPRResult, MountFd, MountFds};

/// An arbitrary maximum size for CFileHandle::f_handle.
///
/// According to Linux ABI, struct file_handle has a flexible array member 'f_handle', with
/// maximum value of 128 bytes defined in file include/linux/exportfs.h
pub const MAX_HANDLE_SIZE: usize = 128;

/// Dynamically allocated array.
#[derive(Default)]
#[repr(C)]
pub struct __IncompleteArrayField<T>(::std::marker::PhantomData<T>, [T; 0]);
impl<T> __IncompleteArrayField<T> {
    #[inline]
    pub fn as_ptr(&self) -> *const T {
        self as *const __IncompleteArrayField<T> as *const T
    }
    #[inline]
    pub fn as_mut_ptr(&mut self) -> *mut T {
        self as *mut __IncompleteArrayField<T> as *mut T
    }
    #[inline]
    pub fn as_slice(&self, len: usize) -> &[T] {
        unsafe { ::std::slice::from_raw_parts(self.as_ptr(), len) }
    }
    #[inline]
    pub fn as_mut_slice(&mut self, len: usize) -> &mut [T] {
        unsafe { ::std::slice::from_raw_parts_mut(self.as_mut_ptr(), len) }
    }
}

/// The structure to transfer file_handle struct between user space and kernel space.
/// ```c
/// struct file_handle {
///     __u32 handle_bytes;
///     int handle_type;
///     /* file identifier */
///     unsigned char f_handle[];
/// }
/// ```
#[derive(Default)]
#[repr(C)]
pub struct CFileHandleInner {
    pub handle_bytes: libc::c_uint,
    pub handle_type: libc::c_int,
    pub f_handle: __IncompleteArrayField<libc::c_char>,
}

vmm_sys_util::generate_fam_struct_impl!(
    CFileHandleInner,
    libc::c_char,
    f_handle,
    libc::c_uint,
    handle_bytes,
    MAX_HANDLE_SIZE
);

type CFileHandleWrapper = FamStructWrapper<CFileHandleInner>;

#[derive(Clone)]
struct CFileHandle {
    pub wrapper: CFileHandleWrapper,
}

impl CFileHandle {
    fn new(size: usize) -> Self {
        CFileHandle {
            wrapper: CFileHandleWrapper::new(size).unwrap(),
        }
    }
}

// Safe because f_handle is readonly once FileHandle is initialized.
unsafe impl Send for CFileHandle {}
unsafe impl Sync for CFileHandle {}

impl Ord for CFileHandle {
    fn cmp(&self, other: &Self) -> Ordering {
        let s_fh = self.wrapper.as_fam_struct_ref();
        let o_fh = other.wrapper.as_fam_struct_ref();
        if s_fh.handle_bytes != o_fh.handle_bytes {
            return s_fh.handle_bytes.cmp(&o_fh.handle_bytes);
        }
        let length = s_fh.handle_bytes as usize;
        if s_fh.handle_type != o_fh.handle_type {
            return s_fh.handle_type.cmp(&o_fh.handle_type);
        }

        if s_fh.f_handle.as_ptr() != o_fh.f_handle.as_ptr() {
            return s_fh
                .f_handle
                .as_slice(length)
                .cmp(o_fh.f_handle.as_slice(length));
        }

        Ordering::Equal
    }
}

impl PartialOrd for CFileHandle {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for CFileHandle {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for CFileHandle {}

impl Debug for CFileHandle {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let fh = self.wrapper.as_fam_struct_ref();
        write!(
            f,
            "File handle: type {}, len {}, inner {:?}",
            fh.handle_type,
            fh.handle_bytes,
            fh.f_handle.as_slice(fh.handle_bytes as usize)
        )
    }
}

/// Struct to maintain information for a file handle.
#[derive(Clone, PartialOrd, Ord, PartialEq, Eq, Debug)]
pub struct FileHandle {
    pub(crate) mnt_id: u64,
    handle: CFileHandle,
}

impl Default for FileHandle {
    fn default() -> Self {
        Self {
            mnt_id: 0,
            handle: CFileHandle::new(0),
        }
    }
}

unsafe extern "C" {
    #[allow(dead_code)]
    unsafe fn name_to_handle_at(
        dirfd: libc::c_int,
        pathname: *const libc::c_char,
        file_handle: *mut CFileHandleInner,
        mount_id: *mut libc::c_int,
        flags: libc::c_int,
    ) -> libc::c_int;

    // Technically `file_handle` should be a `mut` pointer, but `open_by_handle_at()` is specified
    // not to change it, so we can declare it `const`.
    #[allow(dead_code)]
    unsafe fn open_by_handle_at(
        mount_fd: libc::c_int,
        file_handle: *const CFileHandleInner,
        flags: libc::c_int,
    ) -> libc::c_int;
}

impl FileHandle {
    /// Create a file handle for the given file.
    ///
    /// Return `Ok(None)` if no file handle can be generated for this file: Either because the
    /// filesystem does not support it, or because it would require a larger file handle than we
    /// can store.  These are not intermittent failures, i.e. if this function returns `Ok(None)`
    /// for a specific file, it will always return `Ok(None)` for it.  Conversely, if this function
    /// returns `Ok(Some)` at some point, it will never return `Ok(None)` later.
    ///
    /// Return an `io::Error` for all other errors.
    pub fn from_name_at(
        #[allow(unused_variables)] dir_fd: &impl AsRawFd,
        #[allow(unused_variables)] path: &CStr,
    ) -> io::Result<Option<Self>> {
        #[cfg(target_os = "linux")]
        {
            let mut mount_id: libc::c_int = 0;
            let mut c_fh = CFileHandle::new(0);

            // Per name_to_handle_at(2), the caller can discover the required size
            // for the file_handle structure by making a call in which
            // handle->handle_bytes is zero.  In this case, the call fails with the
            // error EOVERFLOW and handle->handle_bytes is set to indicate the
            // required size; the caller can then use this information to allocate a
            // structure of the correct size.
            let ret = unsafe {
                name_to_handle_at(
                    dir_fd.as_raw_fd(),
                    path.as_ptr(),
                    c_fh.wrapper.as_mut_fam_struct_ptr(),
                    &mut mount_id,
                    libc::AT_EMPTY_PATH,
                )
            };
            if ret == -1 {
                let err = io::Error::last_os_error();
                match err.raw_os_error() {
                    // Got the needed buffer size.
                    Some(libc::EOVERFLOW) => {}
                    // Filesystem does not support file handles
                    Some(libc::EOPNOTSUPP) => return Ok(None),
                    // Other error
                    _ => return Err(err),
                }
            } else {
                return Err(io::Error::from(io::ErrorKind::InvalidData));
            }

            let needed = c_fh.wrapper.as_fam_struct_ref().handle_bytes as usize;
            let mut c_fh = CFileHandle::new(needed);

            // name_to_handle_at() does not trigger a mount when the final component of the pathname is
            // an automount point. When a filesystem supports both file handles and automount points,
            // a name_to_handle_at() call on an automount point will return with error EOVERFLOW
            // without having increased handle_bytes.  This can happen since Linux 4.13 with NFS
            // when accessing a directory which is on a separate filesystem on the server. In this case,
            // the automount can be triggered by adding a "/" to the end of the pathname.
            let ret = unsafe {
                name_to_handle_at(
                    dir_fd.as_raw_fd(),
                    path.as_ptr(),
                    c_fh.wrapper.as_mut_fam_struct_ptr(),
                    &mut mount_id,
                    libc::AT_EMPTY_PATH,
                )
            };
            if ret == -1 {
                return Err(io::Error::last_os_error());
            }

            Ok(Some(FileHandle {
                mnt_id: mount_id as MountId,
                handle: c_fh,
            }))
        }
        #[cfg(target_os = "macos")]
        {
            Ok(None)
        }
    }

    /// Create a file handle from a `fd`.
    /// This is a wrapper around `from_name_at()` and so has the same interface.
    pub fn from_fd(fd: &impl AsRawFd) -> io::Result<Option<Self>> {
        // Safe because this is a constant value and a valid C string.
        let empty_path = unsafe { CStr::from_bytes_with_nul_unchecked(EMPTY_CSTR) };
        Self::from_name_at(fd, empty_path)
    }

    /// Return an openable copy of the file handle by ensuring that `mount_fd` contains a valid fd
    /// for the mount the file handle is for.
    ///
    /// `reopen_fd` will be invoked to duplicate an `O_PATH` fd with custom `libc::open()` flags.
    pub fn into_openable<F>(
        self,
        mount_fds: &MountFds,
        reopen_fd: F,
    ) -> MPRResult<OpenableFileHandle>
    where
        F: FnOnce(RawFd, libc::c_int, u32) -> io::Result<File>,
    {
        let mount_fd = mount_fds.get(self.mnt_id, reopen_fd)?;
        Ok(OpenableFileHandle {
            handle: Arc::new(self),
            mount_fd,
        })
    }
}

pub struct OpenableFileHandle {
    handle: Arc<FileHandle>,
    mount_fd: Arc<MountFd>,
}

impl Debug for OpenableFileHandle {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let fh = self.handle.handle.wrapper.as_fam_struct_ref();
        write!(
            f,
            "Openable file handle: mountfd {}, type {}, len {}",
            self.mount_fd.as_fd().as_raw_fd(),
            fh.handle_type,
            fh.handle_bytes
        )
    }
}

impl OpenableFileHandle {
    /// Open a file from an openable file handle.
    pub fn open(&self, #[allow(unused_variables)] flags: libc::c_int) -> io::Result<File> {
        #[cfg(target_os = "linux")]
        {
            let ret = unsafe {
                open_by_handle_at(
                    self.mount_fd.as_fd().as_raw_fd(),
                    self.handle.handle.wrapper.as_fam_struct_ptr(),
                    flags,
                )
            };
            if ret >= 0 {
                // Safe because `open_by_handle_at()` guarantees this is a valid fd
                let file = unsafe { File::from_raw_fd(ret) };
                Ok(file)
            } else {
                let e = io::Error::last_os_error();
                error!("open_by_handle_at failed error {e:?}");
                Err(e)
            }
        }
        #[cfg(target_os = "macos")]
        {
            Err(io::Error::from_raw_os_error(libc::ENOTSUP))
        }
    }

    pub fn file_handle(&self) -> &Arc<FileHandle> {
        &self.handle
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_os = "linux")]
    use std::ffi::CString;
    #[cfg(target_os = "linux")]
    use std::fs::OpenOptions;

    fn generate_c_file_handle(
        handle_bytes: usize,
        handle_type: libc::c_int,
        buf: Vec<libc::c_char>,
    ) -> CFileHandle {
        let mut wrapper = CFileHandle::new(handle_bytes);
        let fh = unsafe { wrapper.wrapper.as_mut_fam_struct() };
        fh.handle_type = handle_type;

        fh.f_handle
            .as_mut_slice(handle_bytes)
            .copy_from_slice(buf.as_slice());

        wrapper
    }

    #[test]
    fn test_file_handle_derives() {
        let h1 = generate_c_file_handle(128, 3, vec![0; 128]);
        let mut fh1 = FileHandle {
            mnt_id: 0,
            handle: h1,
        };

        let h2 = generate_c_file_handle(127, 3, vec![0; 127]);
        let fh2 = FileHandle {
            mnt_id: 0,
            handle: h2,
        };

        let h3 = generate_c_file_handle(128, 4, vec![0; 128]);
        let fh3 = FileHandle {
            mnt_id: 0,
            handle: h3,
        };

        let h4 = generate_c_file_handle(128, 3, vec![1; 128]);
        let fh4 = FileHandle {
            mnt_id: 0,
            handle: h4,
        };

        let h5 = generate_c_file_handle(128, 3, vec![0; 128]);
        let mut fh5 = FileHandle {
            mnt_id: 0,
            handle: h5,
        };

        assert!(fh1 > fh2);
        assert_ne!(fh1, fh2);
        assert!(fh1 < fh3);
        assert_ne!(fh1, fh3);
        assert!(fh1 < fh4);
        assert_ne!(fh1, fh4);
        assert_eq!(fh1, fh5);

        unsafe {
            fh1.handle
                .wrapper
                .as_mut_fam_struct()
                .f_handle
                .as_mut_slice(128)[0] = 1;
        }
        assert!(fh1 > fh5);
        unsafe {
            fh5.handle
                .wrapper
                .as_mut_fam_struct()
                .f_handle
                .as_mut_slice(128)[0] = 1;
        }
        assert_eq!(fh1, fh5);
    }

    #[test]
    fn test_c_file_handle_wrapper() {
        let buf = (0..=127).collect::<Vec<libc::c_char>>();
        let mut wrapper = generate_c_file_handle(MAX_HANDLE_SIZE, 3, buf.clone());
        let fh = unsafe { wrapper.wrapper.as_mut_fam_struct() };

        assert_eq!(fh.handle_bytes as usize, MAX_HANDLE_SIZE);
        assert_eq!(fh.handle_type, 3);
        assert_eq!(fh.f_handle.as_slice(MAX_HANDLE_SIZE), buf.as_slice(),);
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_file_handle_from_name_at() {
        // Create a temporary file in /tmp
        let tmp_dir = std::env::temp_dir();
        let tmp_file_path = tmp_dir.join("build.rs");
        let _tmp_file = OpenOptions::new()
            .truncate(true)
            .create(true)
            .write(true)
            .open(&tmp_file_path)
            .unwrap();

        let dir = File::open(tmp_dir).unwrap();
        let filename = CString::new("build.rs").unwrap();

        let dir_handle = FileHandle::from_name_at(&dir, &CString::new("").unwrap())
            .unwrap()
            .unwrap();
        let file_handle = FileHandle::from_name_at(&dir, &filename).unwrap().unwrap();

        assert_eq!(dir_handle.mnt_id, file_handle.mnt_id);
        assert_ne!(
            dir_handle.handle.wrapper.as_fam_struct_ref().handle_bytes,
            0
        );
        assert_ne!(
            file_handle.handle.wrapper.as_fam_struct_ref().handle_bytes,
            0
        );

        // Clean up the temporary file
        std::fs::remove_file(tmp_file_path).unwrap();
    }
}
