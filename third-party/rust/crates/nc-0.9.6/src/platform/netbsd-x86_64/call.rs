// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_safety_doc)]
#![allow(clippy::similar_names)]
#![allow(clippy::wildcard_imports)]

extern crate alloc;

use crate::c_str::CString;
use crate::path::Path;
use crate::syscalls::*;
use crate::sysno::*;
use crate::types::*;

/// Accept a connection on a socket.
///
/// On success, `accept()` return a file descriptor for the accepted socket.
pub unsafe fn accept(
    sockfd: i32,
    addr: &mut sockaddr_in_t,
    addrlen: &mut socklen_t,
) -> Result<i32, Errno> {
    let sockfd = sockfd as usize;
    let addr_ptr = addr as *mut sockaddr_in_t as usize;
    let addrlen_ptr = addrlen as *mut socklen_t as usize;
    syscall3(SYS_ACCEPT, sockfd, addr_ptr, addrlen_ptr).map(|val| val as i32)
}

/// Check user's permission for a file.
///
/// It uses the real user ID and the group access list to authorize the request.
///
/// # Example
///
/// ```
/// let ret = unsafe { nc::access("/etc/passwd", nc::F_OK) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::access("/etc/passwd", nc::X_OK) };
/// assert!(ret.is_err());
/// ```
pub unsafe fn access<P: AsRef<Path>>(filename: P, mode: i32) -> Result<(), Errno> {
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    let mode = mode as usize;
    syscall2(SYS_ACCESS, filename_ptr, mode).map(drop)
}

/// Switch process accounting.
///
/// # Example
///
/// ```
/// let path = "/tmp/nc-acct";
/// let fd = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_WRONLY | nc::O_CREAT, 0o644) };
/// assert!(fd.is_ok());
/// let fd = fd.unwrap();
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::acct(path) };
/// assert_eq!(ret, Err(nc::EPERM));
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, path, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn acct<P: AsRef<Path>>(filename: P) -> Result<(), Errno> {
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    syscall1(SYS_ACCT, filename_ptr).map(drop)
}

/// Cancel an outstanding asynchronous I/O operation (REALTIME)
pub unsafe fn aio_cancel(fd: i32, job: &mut aiocb_t) -> Result<i32, Errno> {
    let fd = fd as usize;
    let job_ptr = job as *mut aiocb_t as usize;
    syscall2(SYS_AIO_CANCEL, fd, job_ptr).map(|val| val as i32)
}

/// Retrieve error status of asynchronous I/O operation
pub unsafe fn aio_error(job: &aiocb_t) -> Result<(), Errno> {
    let job_ptr = job as *const aiocb_t as usize;
    syscall1(SYS_AIO_ERROR, job_ptr).map(drop)
}

/// Asynchronous file synchronization (REALTIME)
pub unsafe fn aio_fsync(op: i32, job: &mut aiocb_t) -> Result<(), Errno> {
    let op = op as usize;
    let job_ptr = job as *mut aiocb_t as usize;
    syscall2(SYS_AIO_FSYNC, op, job_ptr).map(drop)
}

/// Asynchronous read from a file (REALTIME)
pub unsafe fn aio_read(job: &mut aiocb_t) -> Result<(), Errno> {
    let job_ptr = job as *mut aiocb_t as usize;
    syscall1(SYS_AIO_READ, job_ptr).map(drop)
}

/// Retrieve return status of asynchronous I/O operation (REALTIME)
pub unsafe fn aio_return(job: &mut aiocb_t) -> Result<ssize_t, Errno> {
    let job_ptr = job as *mut aiocb_t as usize;
    syscall1(SYS_AIO_RETURN, job_ptr).map(|val| val as ssize_t)
}

/// Asynchronous write to a file (REALTIME)
pub unsafe fn aio_write(job: &mut aiocb_t) -> Result<(), Errno> {
    let job_ptr = job as *mut aiocb_t as usize;
    syscall1(SYS_AIO_WRITE, job_ptr).map(drop)
}

/// Bind a name to a socket.
pub unsafe fn bind(sockfd: i32, addr: &sockaddr_t, addrlen: socklen_t) -> Result<(), Errno> {
    let sockfd = sockfd as usize;
    let addr_ptr = addr as *const sockaddr_t as usize;
    let addrlen = addrlen as usize;
    syscall3(SYS_BIND, sockfd, addr_ptr, addrlen).map(drop)
}

pub unsafe fn r#break(addr: usize) -> Result<(), Errno> {
    syscall1(SYS_BREAK, addr).map(drop)
}

/// Change working directory.
///
/// # Example
///
/// ```
/// let path = "/tmp";
/// // Open folder directly.
/// let ret = unsafe { nc::chdir(path) };
/// assert!(ret.is_ok());
///
/// let mut buf = [0_u8; nc::PATH_MAX as usize + 1];
/// let ret = unsafe { nc::getcwd(buf.as_mut_ptr() as usize, buf.len()) };
/// assert!(ret.is_ok());
/// // Remove null-terminal char.
/// let path_len = ret.unwrap() as usize - 1;
/// let new_cwd = std::str::from_utf8(&buf[..path_len]);
/// assert_eq!(new_cwd, Ok(path));
/// ```
pub unsafe fn chdir<P: AsRef<Path>>(filename: P) -> Result<(), Errno> {
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    syscall1(SYS_CHDIR, filename_ptr).map(drop)
}

/// Set file flags.
pub unsafe fn chflags<P: AsRef<Path>>(path: P, flags: fflags_t) -> Result<(), Errno> {
    let path = CString::new(path.as_ref());
    let path_ptr = path.as_ptr() as usize;
    let flags = flags as usize;
    syscall2(SYS_CHFLAGS, path_ptr, flags).map(drop)
}

/// Change permissions of a file.
///
/// # Example
///
/// ```
/// let filename = "/tmp/nc-chmod";
/// let fd = unsafe {
///     nc::openat(
///         nc::AT_FDCWD,
///         filename,
///         nc::O_CREAT | nc::O_WRONLY | nc::O_TRUNC,
///         0o644,
///     )
/// };
/// assert!(fd.is_ok());
/// let fd = fd.unwrap();
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::chmod(filename, 0o600) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, filename, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn chmod<P: AsRef<Path>>(filename: P, mode: mode_t) -> Result<(), Errno> {
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    let mode = mode as usize;
    syscall2(SYS_CHMOD, filename_ptr, mode).map(drop)
}

/// Change ownership of a file.
///
/// # Example
///
/// ```
/// let filename = "/tmp/nc-chown";
/// let fd = unsafe { nc::openat(nc::AT_FDCWD, filename, nc::O_CREAT | nc::O_WRONLY | nc::O_TRUNC, 0o644) };
/// assert!(fd.is_ok());
/// let fd = fd.unwrap();
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::chown(filename, 0, 0) };
/// assert!(ret.is_err());
/// assert_eq!(ret, Err(nc::EPERM));
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, filename, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn chown<P: AsRef<Path>>(filename: P, user: uid_t, group: gid_t) -> Result<(), Errno> {
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    let user = user as usize;
    let group = group as usize;
    syscall3(SYS_CHOWN, filename_ptr, user, group).map(drop)
}

/// Change the root directory.
///
/// # Example
///
/// ```
/// let ret = unsafe { nc::chroot("/") };
/// assert_eq!(ret, Err(nc::EPERM));
/// ```
pub unsafe fn chroot<P: AsRef<Path>>(filename: P) -> Result<(), Errno> {
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    syscall1(SYS_CHROOT, filename_ptr).map(drop)
}

/// Obtain ID of a process CPU-time clock.
pub unsafe fn clock_getcpuclockid2(
    id: id_t,
    which: i32,
    clock_id: &mut clockid_t,
) -> Result<(), Errno> {
    let id = id as usize;
    let which = which as usize;
    let clock_id_ptr = clock_id as *mut clockid_t as usize;
    syscall3(SYS_CLOCK_GETCPUCLOCKID2, id, which, clock_id_ptr).map(drop)
}

/// High resolution sleep with a specific clock.
///
/// # Example
///
/// ```
/// let t = nc::timespec_t {
///     tv_sec: 1,
///     tv_nsec: 0,
/// };
/// let mut rem = nc::timespec_t::default();
/// let ret = unsafe { nc::clock_nanosleep(nc::CLOCK_MONOTONIC, 0, &t, &mut rem) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn clock_nanosleep(
    which_clock: clockid_t,
    flags: i32,
    rqtp: &timespec_t,
    rmtp: &mut timespec_t,
) -> Result<(), Errno> {
    let which_clock = which_clock as usize;
    let flags = flags as usize;
    let rqtp_ptr = rqtp as *const timespec_t as usize;
    let rmtp_ptr = rmtp as *mut timespec_t as usize;
    syscall4(SYS_CLOCK_NANOSLEEP, which_clock, flags, rqtp_ptr, rmtp_ptr).map(drop)
}

/// Close a file descriptor.
///
/// # Example
///
/// ```
/// const STDERR_FD: i32 = 2;
/// let ret = unsafe { nc::close(STDERR_FD) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn close(fd: i32) -> Result<(), Errno> {
    let fd = fd as usize;
    syscall1(SYS_CLOSE, fd).map(drop)
}

/// Initialize a connection on a socket.
pub unsafe fn connect(
    sockfd: i32,
    addr: *const sockaddr_t,
    addrlen: socklen_t,
) -> Result<(), Errno> {
    let sockfd = sockfd as usize;
    let addr_ptr = addr as usize;
    let addrlen = addrlen as usize;
    syscall3(SYS_CONNECT, sockfd, addr_ptr, addrlen).map(drop)
}

/// Create a copy of the file descriptor `oldfd`, using the lowest available
/// file descriptor.
///
/// # Example
///
/// ```
/// let path = "/tmp/nc-dup-file";
/// let fd = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_CREAT | nc::O_WRONLY | nc::O_TRUNC, 0o644) };
/// assert!(fd.is_ok());
/// let fd = fd.unwrap();
/// let fd_dup = unsafe { nc::dup(fd) };
/// assert!(fd_dup.is_ok());
/// let fd_dup = fd_dup.unwrap();
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::close(fd_dup) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, path, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn dup(oldfd: i32) -> Result<i32, Errno> {
    let oldfd = oldfd as usize;
    syscall1(SYS_DUP, oldfd).map(|ret| ret as i32)
}

/// Create a copy of the file descriptor `oldfd`, using the speficified file
/// descriptor `newfd`.
///
/// # Example
///
/// ```
/// let path = "/tmp/nc-dup2-file";
/// let fd = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_CREAT | nc::O_WRONLY | nc::O_TRUNC, 0o644) };
/// assert!(fd.is_ok());
/// let fd = fd.unwrap();
/// let newfd = 8;
/// let ret = unsafe { nc::dup2(fd, newfd) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::close(newfd) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, path, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn dup2(oldfd: i32, newfd: i32) -> Result<(), Errno> {
    let oldfd = oldfd as usize;
    let newfd = newfd as usize;
    syscall2(SYS_DUP2, oldfd, newfd).map(drop)
}

/// Save as `dup2()`, but can set the close-on-exec flag on `newfd`.
///
/// # Example
///
/// ```
/// let path = "/tmp/nc-dup3-file";
/// let fd = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_CREAT | nc::O_WRONLY | nc::O_TRUNC, 0o644) };
/// assert!(fd.is_ok());
/// let fd = fd.unwrap();
/// let newfd = 8;
/// let ret = unsafe { nc::dup3(fd, newfd, nc::O_CLOEXEC) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::close(newfd) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, path, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn dup3(oldfd: i32, newfd: i32, flags: i32) -> Result<(), Errno> {
    let oldfd = oldfd as usize;
    let newfd = newfd as usize;
    let flags = flags as usize;
    syscall3(SYS_DUP3, oldfd, newfd, flags).map(drop)
}

/// Execute a new program.
///
/// TODO(Shaohua): type of argv and env will be changed.
/// And return value might be changed too.
///
/// # Example
///
/// ```
/// let args = [""];
/// let env = [""];
/// let ret = unsafe { nc::execve("/bin/ls", &args, &env) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn execve<P: AsRef<Path>>(
    filename: P,
    argv: &[&str],
    env: &[&str],
) -> Result<(), Errno> {
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    let argv_ptr = argv.as_ptr() as usize;
    let env_ptr = env.as_ptr() as usize;
    syscall3(SYS_EXECVE, filename_ptr, argv_ptr, env_ptr).map(drop)
}

/// Terminate current process.
///
/// # Example
///
/// ```
/// unsafe { nc::exit(0); }
/// ```
pub unsafe fn exit(status: i32) -> ! {
    let status = status as usize;
    let _ret = syscall1(SYS_EXIT, status);
    unreachable!();
}

/// Manage UFS1 extended attributes
pub unsafe fn extattrctl<P: AsRef<Path>>(
    path: P,
    cmd: i32,
    filename: P,
    attr_namespace: i32,
    attr_name: &str,
) -> Result<(), Errno> {
    let path = CString::new(path.as_ref());
    let path_ptr = path.as_ptr() as usize;
    let cmd = cmd as usize;
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    let attr_namespace = attr_namespace as usize;
    let attr_name = CString::new(attr_name);
    let attr_name_ptr = attr_name.as_ptr() as usize;
    syscall5(
        SYS_EXTATTRCTL,
        path_ptr,
        cmd,
        filename_ptr,
        attr_namespace,
        attr_name_ptr,
    )
    .map(drop)
}

/// Deletes the VFS extended attribute specified.
pub unsafe fn extattr_delete_fd(
    fd: i32,
    attr_namespace: i32,
    attr_name: &str,
) -> Result<(), Errno> {
    let fd = fd as usize;
    let attr_namespace = attr_namespace as usize;
    let attr_name_ptr = attr_name.as_ptr() as usize;
    syscall3(SYS_EXTATTR_DELETE_FD, fd, attr_namespace, attr_name_ptr).map(drop)
}

/// Deletes the VFS extended attribute specified.
pub unsafe fn extattr_delete_file<P: AsRef<Path>>(
    path: P,
    attr_namespace: i32,
    attr_name: &str,
) -> Result<(), Errno> {
    let path = CString::new(path.as_ref());
    let path_ptr = path.as_ptr() as usize;
    let attr_namespace = attr_namespace as usize;
    let attr_name_ptr = attr_name.as_ptr() as usize;
    syscall3(
        SYS_EXTATTR_DELETE_FILE,
        path_ptr,
        attr_namespace,
        attr_name_ptr,
    )
    .map(drop)
}

/// Deletes the VFS extended attribute specified, without following symlinks.
pub unsafe fn extattr_delete_link<P: AsRef<Path>>(
    path: P,
    attr_namespace: i32,
    attr_name: &str,
) -> Result<(), Errno> {
    let path = CString::new(path.as_ref());
    let path_ptr = path.as_ptr() as usize;
    let attr_namespace = attr_namespace as usize;
    let attr_name_ptr = attr_name.as_ptr() as usize;
    syscall3(
        SYS_EXTATTR_DELETE_LINK,
        path_ptr,
        attr_namespace,
        attr_name_ptr,
    )
    .map(drop)
}

/// Get the value of the VFS extended attribute specified.
pub unsafe fn extattr_get_fd(
    fd: i32,
    attr_namespace: i32,
    attr_name: &str,
    data: &mut [u8],
) -> Result<ssize_t, Errno> {
    let fd = fd as usize;
    let attr_namespace = attr_namespace as usize;
    let attr_name_ptr = attr_name.as_ptr() as usize;
    let data_ptr = data.as_mut_ptr() as usize;
    let nbytes = data.len();
    syscall5(
        SYS_EXTATTR_GET_FILE,
        fd,
        attr_namespace,
        attr_name_ptr,
        data_ptr,
        nbytes,
    )
    .map(|val| val as ssize_t)
}

/// Get the value of the VFS extended attribute specified.
pub unsafe fn extattr_get_file<P: AsRef<Path>>(
    path: P,
    attr_namespace: i32,
    attr_name: &str,
    data: &mut [u8],
) -> Result<ssize_t, Errno> {
    let path = CString::new(path.as_ref());
    let path_ptr = path.as_ptr() as usize;
    let attr_namespace = attr_namespace as usize;
    let attr_name = CString::new(attr_name);
    let attr_name_ptr = attr_name.as_ptr() as usize;
    let data_ptr = data.as_mut_ptr() as usize;
    let nbytes = data.len();
    syscall5(
        SYS_EXTATTR_GET_FILE,
        path_ptr,
        attr_namespace,
        attr_name_ptr,
        data_ptr,
        nbytes,
    )
    .map(|val| val as ssize_t)
}

/// Get the value of the VFS extended attribute specified, withoug following symlinks.
pub unsafe fn extattr_get_link<P: AsRef<Path>>(
    path: P,
    attr_namespace: i32,
    attr_name: &str,
    data: &mut [u8],
) -> Result<ssize_t, Errno> {
    let path = CString::new(path.as_ref());
    let path_ptr = path.as_ptr() as usize;
    let attr_namespace = attr_namespace as usize;
    let attr_name_ptr = attr_name.as_ptr() as usize;
    let data_ptr = data.as_mut_ptr() as usize;
    let nbytes = data.len();
    syscall5(
        SYS_EXTATTR_GET_LINK,
        path_ptr,
        attr_namespace,
        attr_name_ptr,
        data_ptr,
        nbytes,
    )
    .map(|val| val as ssize_t)
}

/// Returns a list of the VFS extended attributes present in the requested namespace.
pub unsafe fn extattr_list_fd(
    fd: i32,
    attr_namespace: i32,
    data: &mut [u8],
) -> Result<ssize_t, Errno> {
    let fd = fd as usize;
    let attr_namespace = attr_namespace as usize;
    let data_ptr = data.as_mut_ptr() as usize;
    let nbytes = data.len();
    syscall4(SYS_EXTATTR_LIST_FD, fd, attr_namespace, data_ptr, nbytes).map(|val| val as ssize_t)
}

/// Returns a list of the VFS extended attributes present in the requested namespace.
pub unsafe fn extattr_list_file<P: AsRef<Path>>(
    path: P,
    attr_namespace: i32,
    data: &mut [u8],
) -> Result<ssize_t, Errno> {
    let path = CString::new(path.as_ref());
    let path_ptr = path.as_ptr() as usize;
    let attr_namespace = attr_namespace as usize;
    let data_ptr = data.as_mut_ptr() as usize;
    let nbytes = data.len();
    syscall4(
        SYS_EXTATTR_LIST_FILE,
        path_ptr,
        attr_namespace,
        data_ptr,
        nbytes,
    )
    .map(|val| val as ssize_t)
}

/// Returns a list of the VFS extended attributes present in the requested namespace,
/// without following symlinks.
pub unsafe fn extattr_list_link<P: AsRef<Path>>(
    path: P,
    attr_namespace: i32,
    data: &mut [u8],
) -> Result<ssize_t, Errno> {
    let path = CString::new(path.as_ref());
    let path_ptr = path.as_ptr() as usize;
    let attr_namespace = attr_namespace as usize;
    let data_ptr = data.as_mut_ptr() as usize;
    let nbytes = data.len();
    syscall4(
        SYS_EXTATTR_LIST_LINK,
        path_ptr,
        attr_namespace,
        data_ptr,
        nbytes,
    )
    .map(|val| val as ssize_t)
}

/// Set the value of the VFS extended attribute specified.
pub unsafe fn extattr_set_fd(
    fd: i32,
    attr_namespace: i32,
    attr_name: &str,
    data: &[u8],
) -> Result<ssize_t, Errno> {
    let fd = fd as usize;
    let attr_namespace = attr_namespace as usize;
    let attr_name_ptr = attr_name.as_ptr() as usize;
    let data_ptr = data.as_ptr() as usize;
    let nbytes = data.len();
    syscall5(
        SYS_EXTATTR_SET_FD,
        fd,
        attr_namespace,
        attr_name_ptr,
        data_ptr,
        nbytes,
    )
    .map(|val| val as ssize_t)
}

/// Set the value of the VFS extended attribute specified.
pub unsafe fn extattr_set_file<P: AsRef<Path>>(
    path: P,
    attr_namespace: i32,
    attr_name: &str,
    data: &[u8],
) -> Result<ssize_t, Errno> {
    let path = CString::new(path.as_ref());
    let path_ptr = path.as_ptr() as usize;
    let attr_namespace = attr_namespace as usize;
    let attr_name = CString::new(attr_name);
    let attr_name_ptr = attr_name.as_ptr() as usize;
    let data_ptr = data.as_ptr() as usize;
    let nbytes = data.len();
    syscall5(
        SYS_EXTATTR_SET_FILE,
        path_ptr,
        attr_namespace,
        attr_name_ptr,
        data_ptr,
        nbytes,
    )
    .map(|val| val as ssize_t)
}

/// Set the value of the VFS extended attribute specified, without following symlinks.
pub unsafe fn extattr_set_link<P: AsRef<Path>>(
    path: P,
    attr_namespace: i32,
    attr_name: &str,
    data: &[u8],
) -> Result<ssize_t, Errno> {
    let path = CString::new(path.as_ref());
    let path_ptr = path.as_ptr() as usize;
    let attr_namespace = attr_namespace as usize;
    let attr_name_ptr = attr_name.as_ptr() as usize;
    let data_ptr = data.as_ptr() as usize;
    let nbytes = data.len();
    syscall5(
        SYS_EXTATTR_SET_LINK,
        path_ptr,
        attr_namespace,
        attr_name_ptr,
        data_ptr,
        nbytes,
    )
    .map(|val| val as ssize_t)
}

/// Check user's permission for a file.
///
/// # Example
///
/// ```
/// let ret = unsafe { nc::faccessat(nc::AT_FDCWD, "/etc/passwd", nc::F_OK) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn faccessat<P: AsRef<Path>>(dfd: i32, filename: P, mode: i32) -> Result<(), Errno> {
    let dfd = dfd as usize;
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    let mode = mode as usize;
    syscall3(SYS_FACCESSAT, dfd, filename_ptr, mode).map(drop)
}

/// Change working directory.
///
/// # Example
///
/// ```
/// let path = "/tmp";
/// // Open folder directly.
/// let fd = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_PATH, 0) };
/// assert!(fd.is_ok());
/// let fd = fd.unwrap();
/// let ret = unsafe { nc::fchdir(fd) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn fchdir(fd: i32) -> Result<(), Errno> {
    let fd = fd as usize;
    syscall1(SYS_FCHDIR, fd).map(drop)
}

/// Set file flags.
pub unsafe fn fchflags(fd: i32, flags: fflags_t) -> Result<(), Errno> {
    let fd = fd as usize;
    let flags = flags as usize;
    syscall2(SYS_FCHFLAGS, fd, flags).map(drop)
}

/// Change permissions of a file.
///
/// # Example
///
/// ```
/// let filename = "/tmp/nc-fchmod";
/// let fd = unsafe {
///     nc::openat(
///         nc::AT_FDCWD,
///         filename,
///         nc::O_CREAT | nc::O_WRONLY | nc::O_TRUNC,
///         0o644
///     )
/// };
/// assert!(fd.is_ok());
/// let fd = fd.unwrap();
/// let ret = unsafe { nc::fchmod(fd, 0o600) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, filename, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn fchmod(fd: i32, mode: mode_t) -> Result<(), Errno> {
    let fd = fd as usize;
    let mode = mode as usize;
    syscall2(SYS_FCHMOD, fd, mode).map(drop)
}

/// Change permissions of a file.
///
/// # Example
///
/// ```
/// let filename = "/tmp/nc-fchmodat";
/// let fd = unsafe {
///     nc::openat(
///         nc::AT_FDCWD,
///         filename,
///         nc::O_CREAT | nc::O_WRONLY | nc::O_TRUNC,
///         0o644
///     )
/// };
/// assert!(fd.is_ok());
/// let fd = fd.unwrap();
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::fchmodat(nc::AT_FDCWD, filename, 0o600) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, filename, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn fchmodat<P: AsRef<Path>>(dirfd: i32, filename: P, mode: mode_t) -> Result<(), Errno> {
    let dirfd = dirfd as usize;
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    let mode = mode as usize;
    syscall3(SYS_FCHMODAT, dirfd, filename_ptr, mode).map(drop)
}

/// Change ownership of a file.
///
/// # Example
///
/// ```
/// let filename = "/tmp/nc-fchown";
/// let fd = unsafe {
///     nc::openat(
///         nc::AT_FDCWD,
///         filename,
///         nc::O_CREAT | nc::O_WRONLY | nc::O_TRUNC,
///         0o644
///     )
/// };
/// assert!(fd.is_ok());
/// let fd = fd.unwrap();
/// let ret = unsafe { nc::fchown(fd, 0, 0) };
/// assert!(ret.is_err());
/// assert_eq!(ret, Err(nc::EPERM));
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, filename, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn fchown(fd: i32, user: uid_t, group: gid_t) -> Result<(), Errno> {
    let fd = fd as usize;
    let user = user as usize;
    let group = group as usize;
    syscall3(SYS_FCHOWN, fd, user, group).map(drop)
}

/// Change ownership of a file.
///
/// # Example
///
/// ```
/// let filename = "/tmp/nc-fchown";
/// let fd = unsafe {
///     nc::openat(
///         nc::AT_FDCWD,
///         filename,
///         nc::O_CREAT | nc::O_WRONLY | nc::O_TRUNC,
///         0o644
///     )
/// };
/// assert!(fd.is_ok());
/// let fd = fd.unwrap();
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::fchownat(nc::AT_FDCWD, filename, 0, 0, 0) };
/// assert_eq!(ret, Err(nc::EPERM));
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, filename,0 ) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn fchownat<P: AsRef<Path>>(
    dirfd: i32,
    filename: P,
    user: uid_t,
    group: gid_t,
    flag: i32,
) -> Result<(), Errno> {
    let dirfd = dirfd as usize;
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    let user = user as usize;
    let group = group as usize;
    let flag = flag as usize;
    syscall5(SYS_FCHOWNAT, dirfd, filename_ptr, user, group, flag).map(drop)
}

/// Change the root directory.
pub unsafe fn fchroot(fd: i32) -> Result<(), Errno> {
    let fd = fd as usize;
    syscall1(SYS_FCHROOT, fd).map(drop)
}

/// manipulate file descriptor.
///
/// # Example
///
/// ```
/// let path = "/etc/passwd";
/// let ret = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_RDONLY, 0) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
///
/// let ret = unsafe { nc::fcntl(fd, nc::F_DUPFD, 0) };
/// assert!(ret.is_ok());
/// let fd2 = ret.unwrap();
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::close(fd2) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn fcntl(fd: i32, cmd: i32, arg: usize) -> Result<i32, Errno> {
    let fd = fd as usize;
    let cmd = cmd as usize;
    syscall3(SYS_FCNTL, fd, cmd, arg).map(|ret| ret as i32)
}

/// Flush all modified in-core data (exclude metadata) refered by `fd` to disk.
///
/// # Example
///
/// ```
/// let path = "/tmp/nc-fdatasync";
/// let fd = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_WRONLY | nc::O_CREAT, 0o644) };
/// assert!(fd.is_ok());
/// let fd = fd.unwrap();
/// let msg = b"Hello, Rust";
/// let ret = unsafe { nc::write(fd, msg.as_ptr() as usize, msg.len()) };
/// assert!(ret.is_ok());
/// assert_eq!(ret, Ok(msg.len() as nc::ssize_t));
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, path, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn fdatasync(fd: i32) -> Result<(), Errno> {
    let fd = fd as usize;
    syscall1(SYS_FDATASYNC, fd).map(drop)
}

/// Discard backing store for files
pub unsafe fn fdiscard(fd: i32, offset: off_t, len: off_t) -> Result<(), Errno> {
    let fd = fd as usize;
    let offset = offset as usize;
    let len = len as usize;
    syscall3(SYS_FDISCARD, fd, offset, len).map(drop)
}

/// Execute a new program.
pub unsafe fn fexecve(fd: i32, argv: &[&str], env: &[&str]) -> Result<(), Errno> {
    let fd = fd as usize;
    let argv_ptr = argv.as_ptr() as usize;
    let env_ptr = env.as_ptr() as usize;
    syscall3(SYS_FEXECVE, fd, argv_ptr, env_ptr).map(drop)
}

/// Get extended attribute value.
///
/// # Example
///
/// ```
/// let path = "/tmp/nc-fgetxattr";
/// let fd = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_WRONLY | nc::O_CREAT, 0o644) };
/// assert!(fd.is_ok());
/// let fd = fd.unwrap();
/// let attr_name = "user.creator";
/// let attr_value = "nc-0.0.1";
/// //let flags = 0;
/// let flags = nc::XATTR_CREATE;
/// let ret = unsafe {
///     nc::setxattr(
///         path,
///         &attr_name,
///         attr_value.as_ptr() as usize,
///         attr_value.len(),
///         flags,
///     )
/// };
/// assert!(ret.is_ok());
/// let mut buf = [0_u8; 16];
/// let buf_len = buf.len();
/// let ret = unsafe { nc::fgetxattr(fd, attr_name, buf.as_mut_ptr() as usize, buf_len) };
/// assert!(ret.is_ok());
/// assert_eq!(ret, Ok(attr_value.len() as nc::ssize_t));
/// let attr_len = ret.unwrap() as usize;
/// assert_eq!(attr_value.as_bytes(), &buf[..attr_len]);
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, path, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn fgetxattr<P: AsRef<Path>>(
    fd: i32,
    name: P,
    value: usize,
    size: size_t,
) -> Result<ssize_t, Errno> {
    let fd = fd as usize;
    let name = CString::new(name.as_ref());
    let name_ptr = name.as_ptr() as usize;
    syscall4(SYS_FGETXATTR, fd, name_ptr, value, size).map(|ret| ret as ssize_t)
}

/// List extended attribute names.
///
/// # Example
///
/// ```
/// let path = "/tmp/nc-flistxattr";
/// let fd = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_WRONLY | nc::O_CREAT, 0o644) };
/// assert!(fd.is_ok());
/// let fd = fd.unwrap();
/// let attr_name = "user.creator";
/// let attr_value = "nc-0.0.1";
/// //let flags = 0;
/// let flags = nc::XATTR_CREATE;
/// let ret = unsafe {
///     nc::setxattr(
///         path,
///         &attr_name,
///         attr_value.as_ptr() as usize,
///         attr_value.len(),
///         flags,
///     )
/// };
/// assert!(ret.is_ok());
/// let mut buf = [0_u8; 16];
/// let buf_len = buf.len();
/// let ret = unsafe { nc::flistxattr(fd, buf.as_mut_ptr() as usize, buf_len) };
/// let attr_len = ret.unwrap() as usize;
/// assert_eq!(&buf[..attr_len - 1], attr_name.as_bytes());
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, path, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn flistxattr(fd: i32, list: usize, size: size_t) -> Result<ssize_t, Errno> {
    let fd = fd as usize;
    syscall3(SYS_FLISTXATTR, fd, list, size).map(|ret| ret as ssize_t)
}

/// Apply or remove an advisory lock on an open file.
///
/// # Example
///
/// ```
/// let path = "/tmp/nc-flock";
/// let ret = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_WRONLY | nc::O_CREAT, 0o644) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
/// let ret = unsafe { nc::flock(fd, nc::LOCK_EX) };
/// assert!(ret.is_ok());
/// let msg = "Hello, Rust";
/// let ret = unsafe { nc::write(fd, msg.as_ptr() as usize, msg.len()) };
/// assert!(ret.is_ok());
/// assert_eq!(ret, Ok(msg.len() as nc::ssize_t));
/// let ret = unsafe { nc::flock(fd, nc::LOCK_UN) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, path,0 ) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn flock(fd: i32, operation: i32) -> Result<(), Errno> {
    let fd = fd as usize;
    let operation = operation as usize;
    syscall2(SYS_FLOCK, fd, operation).map(drop)
}

/// Create a child process.
///
/// # Example
///
/// ```
/// let pid = unsafe { nc::fork() };
/// assert!(pid.is_ok());
/// let pid = pid.unwrap();
/// assert!(pid >= 0);
/// ```
pub unsafe fn fork() -> Result<pid_t, Errno> {
    syscall0(SYS_FORK).map(|ret| ret as pid_t)
}

/// Get configurable pathname variables
pub unsafe fn fpathconf(fd: i32, name: i32) -> Result<isize, Errno> {
    let fd = fd as usize;
    let name = name as usize;
    syscall2(SYS_FPATHCONF, fd, name).map(|val| val as isize)
}

/// Remove an extended attribute.
///
/// # Example
///
/// ```
/// let path = "/tmp/nc-fremovexattr";
/// let ret = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_WRONLY | nc::O_CREAT, 0o644) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
/// let attr_name = "user.creator";
/// let attr_value = "nc-0.0.1";
/// let flags = nc::XATTR_CREATE;
/// let ret = unsafe {
///     nc::setxattr(
///         path,
///         &attr_name,
///         attr_value.as_ptr() as usize,
///         attr_value.len(),
///         flags,
///     )
/// };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::fremovexattr(fd, attr_name) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, path, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn fremovexattr<P: AsRef<Path>>(fd: i32, name: P) -> Result<(), Errno> {
    let fd = fd as usize;
    let name = CString::new(name.as_ref());
    let name_ptr = name.as_ptr() as usize;
    syscall2(SYS_FREMOVEXATTR, fd, name_ptr).map(drop)
}

/// Set extended attribute value.
///
/// # Example
///
/// ```
/// let path = "/tmp/nc-fsetxattr";
/// let ret = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_WRONLY | nc::O_CREAT, 0o644) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
/// let attr_name = "user.creator";
/// let attr_value = "nc-0.0.1";
/// //let flags = 0;
/// let flags = nc::XATTR_CREATE;
/// let ret = unsafe {
///     nc::fsetxattr(
///         fd,
///         &attr_name,
///         attr_value.as_ptr() as usize,
///         attr_value.len(),
///         flags,
///     )
/// };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, path, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn fsetxattr<P: AsRef<Path>>(
    fd: i32,
    name: P,
    value: usize,
    size: size_t,
    flags: i32,
) -> Result<(), Errno> {
    let fd = fd as usize;
    let name = CString::new(name.as_ref());
    let name_ptr = name.as_ptr() as usize;
    let flags = flags as usize;
    syscall5(SYS_FSETXATTR, fd, name_ptr, value, size, flags).map(drop)
}

/// Get file status.
///
/// # Example
///
/// ```
/// let path = "/etc/passwd";
/// let mut stat = nc::stat_t::default();
/// let ret = unsafe { nc::fstatat(nc::AT_FDCWD, path, &mut stat, nc::AT_SYMLINK_NOFOLLOW) };
/// assert!(ret.is_ok());
/// assert_eq!((stat.st_mode & nc::S_IFMT), nc::S_IFREG);
/// ```
pub unsafe fn fstatat<P: AsRef<Path>>(
    dfd: i32,
    filename: P,
    statbuf: &mut stat_t,
    flag: i32,
) -> Result<(), Errno> {
    let dfd = dfd as usize;
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    let statbuf_ptr = statbuf as *mut stat_t as usize;
    let flag = flag as usize;
    syscall4(SYS_FSTATAT, dfd, filename_ptr, statbuf_ptr, flag).map(drop)
}

/// Get file system statistics
pub unsafe fn fstatvfs1(fd: i32, buf: &mut statvfs_t, flags: i32) -> Result<(), Errno> {
    let fd = fd as usize;
    let buf_ptr = buf as *mut statvfs_t as usize;
    let flags = flags as usize;
    syscall3(SYS_FSTATVFS1, fd, buf_ptr, flags).map(drop)
}

/// Flush all modified in-core data refered by `fd` to disk.
///
/// # Example
///
/// ```
/// let path = "/tmp/nc-fsync";
/// let fd = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_CREAT | nc::O_WRONLY, 0o644) };
/// assert!(fd.is_ok());
/// let fd = fd.unwrap();
/// let buf = b"Hello, Rust";
/// let n_write = unsafe { nc::write(fd, buf.as_ptr() as usize, buf.len()) };
/// assert_eq!(n_write, Ok(buf.len() as isize));
/// let ret = unsafe { nc::fsync(fd) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, path, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn fsync(fd: i32) -> Result<(), Errno> {
    let fd = fd as usize;
    syscall1(SYS_FSYNC, fd).map(drop)
}

/// Flush all modified in-core data refered by `fd` to disk.
pub unsafe fn fsync_range(fd: i32, how: i32, start: off_t, length: off_t) -> Result<(), Errno> {
    let fd = fd as usize;
    let how = how as usize;
    let start = start as usize;
    let length = length as usize;
    syscall4(SYS_FSYNC_RANGE, fd, how, start, length).map(drop)
}

/// Truncate an opened file to a specified length.
///
/// # Example
///
/// ```
/// let path = "/tmp/nc-ftruncate";
/// let ret = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_WRONLY | nc::O_CREAT, 0o644) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
/// let ret = unsafe { nc::ftruncate(fd, 64 * 1024) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, path, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn ftruncate(fd: i32, length: off_t) -> Result<(), Errno> {
    let fd = fd as usize;
    let length = length as usize;
    syscall2(SYS_FTRUNCATE, fd, length).map(drop)
}

/// Change timestamp of a file.
pub unsafe fn futimens(fd: i32, times: &[timespec_t; 2]) -> Result<(), Errno> {
    let fd = fd as usize;
    let times_ptr = times.as_ptr() as usize;
    syscall2(SYS_FUTIMENS, fd, times_ptr).map(drop)
}

/// Get user thread context.
pub unsafe fn getcontext(ctx: &mut ucontext_t) -> Result<(), Errno> {
    let ctx_ptr = ctx as *mut ucontext_t as usize;
    syscall1(SYS_GETCONTEXT, ctx_ptr).map(drop)
}

/// Get the effective group ID of the calling process.
///
/// # Example
///
/// ```
/// let egid = unsafe { nc::getegid() };
/// assert!(egid > 0);
/// ```
#[must_use]
pub unsafe fn getegid() -> gid_t {
    // This function is always successful.
    syscall0(SYS_GETEGID).expect("getegid() failed") as gid_t
}

/// Get the effective user ID of the calling process.
///
/// # Example
///
/// ```
/// let euid = unsafe { nc::geteuid() };
/// assert!(euid > 0);
/// ```
#[must_use]
pub unsafe fn geteuid() -> uid_t {
    // This function is always successful.
    syscall0(SYS_GETEUID).expect("geteuid() failed") as uid_t
}

/// Get the real group ID of the calling process.
///
/// # Example
///
/// ```
/// let gid = unsafe { nc::getgid() };
/// assert!(gid > 0);
/// ```
#[must_use]
pub unsafe fn getgid() -> gid_t {
    // This function is always successful.
    syscall0(SYS_GETGID).expect("getgid() failed") as gid_t
}

/// Get list of supplementary group Ids.
///
/// # Example
///
/// ```
/// let mut groups = vec![];
/// let ret = unsafe { nc::getgroups(0, &mut groups) };
/// assert!(ret.is_ok());
/// let total_num = ret.unwrap();
/// groups.resize(total_num as usize, 0);
///
/// let ret = unsafe { nc::getgroups(total_num, &mut groups) };
/// assert!(ret.is_ok());
/// assert_eq!(ret, Ok(total_num));
/// ```
pub unsafe fn getgroups(size: i32, group_list: &mut [gid_t]) -> Result<i32, Errno> {
    let size = size as usize;
    let group_ptr = group_list.as_mut_ptr() as usize;
    syscall2(SYS_GETGROUPS, size, group_ptr).map(|ret| ret as i32)
}

/// Get name of connected peer socket.
pub unsafe fn getpeername(
    sockfd: i32,
    addr: &mut sockaddr_in_t,
    addrlen: &mut socklen_t,
) -> Result<(), Errno> {
    let sockfd = sockfd as usize;
    let addr_ptr = addr as *mut sockaddr_in_t as usize;
    let addrlen_ptr = addrlen as *mut socklen_t as usize;
    syscall3(SYS_GETPEERNAME, sockfd, addr_ptr, addrlen_ptr).map(drop)
}

/// Returns the PGID(process group ID) of the process specified by `pid`.
///
/// # Example
///
/// ```
/// let ppid = unsafe { nc::getppid() };
/// let pgid = unsafe { nc::getpgid(ppid) };
/// assert!(pgid.is_ok());
/// ```
pub unsafe fn getpgid(pid: pid_t) -> Result<pid_t, Errno> {
    let pid = pid as usize;
    syscall1(SYS_GETPGID, pid).map(|ret| ret as pid_t)
}

/// Get the process group ID of the calling process.
///
/// # Example
///
/// ```
/// let pgroup = unsafe { nc::getpgrp() };
/// assert!(pgroup > 0);
/// ```
#[must_use]
pub unsafe fn getpgrp() -> pid_t {
    // This function is always successful.
    syscall0(SYS_GETPGRP).expect("getpgrp() failed") as pid_t
}

/// Get the process ID (PID) of the calling process.
///
/// # Example
///
/// ```
/// let pid = unsafe { nc::getpid() };
/// assert!(pid > 0);
/// ```
#[must_use]
pub unsafe fn getpid() -> pid_t {
    // This function is always successful.
    syscall0(SYS_GETPID).expect("getpid() failed") as pid_t
}

/// Get the process ID of the parent of the calling process.
///
/// # Example
///
/// ```
/// let ppid = unsafe { nc::getppid() };
/// assert!(ppid > 0);
/// ```
#[must_use]
pub unsafe fn getppid() -> pid_t {
    // This function is always successful.
    syscall0(SYS_GETPPID).expect("getppid() failed") as pid_t
}

/// Get program scheduling priority.
///
/// # Example
///
/// ```
/// let ret = unsafe { nc::getpriority(nc::PRIO_PROCESS, nc::getpid()) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn getpriority(which: i32, who: i32) -> Result<i32, Errno> {
    let which = which as usize;
    let who = who as usize;
    syscall2(SYS_GETPRIORITY, which, who).map(|ret| {
        let ret = ret as i32;
        if ret > PRIO_MAX {
            return PRIO_MAX - ret;
        }
        ret
    })
}

/// Get resource limit.
///
/// # Example
///
/// ```
/// let mut rlimit = nc::rlimit_t::default();
/// let ret = unsafe { nc::getrlimit(nc::RLIMIT_NOFILE, &mut rlimit) };
/// assert!(ret.is_ok());
/// assert!(rlimit.rlim_cur > 0);
/// assert!(rlimit.rlim_max > 0);
/// ```
pub unsafe fn getrlimit(resource: i32, rlim: &mut rlimit_t) -> Result<(), Errno> {
    let resource = resource as usize;
    let rlim_ptr = rlim as *mut rlimit_t as usize;
    syscall2(SYS_GETRLIMIT, resource, rlim_ptr).map(drop)
}

/// Get session Id.
///
/// # Example
///
/// ```
/// let ppid = unsafe { nc::getppid() };
/// let sid = unsafe { nc::getsid(ppid) };
/// assert!(sid > 0);
/// ```
#[must_use]
pub unsafe fn getsid(pid: pid_t) -> pid_t {
    let pid = pid as usize;
    // This function is always successful.
    syscall1(SYS_GETSID, pid).expect("getsid() failed") as pid_t
}

/// Get current address to which the socket `sockfd` is bound.
pub unsafe fn getsockname(
    sockfd: i32,
    addr: &mut sockaddr_in_t,
    addrlen: &mut socklen_t,
) -> Result<(), Errno> {
    let sockfd = sockfd as usize;
    let addr_ptr = addr as *mut sockaddr_in_t as usize;
    let addrlen_ptr = addrlen as *mut socklen_t as usize;
    syscall3(SYS_GETSOCKNAME, sockfd, addr_ptr, addrlen_ptr).map(drop)
}

/// Get options on sockets
pub unsafe fn getsockopt(
    sockfd: i32,
    level: i32,
    optname: i32,
    optval: &mut usize,
    optlen: &mut socklen_t,
) -> Result<(), Errno> {
    let sockfd = sockfd as usize;
    let level = level as usize;
    let optname = optname as usize;
    let optval_ptr = optval as *mut usize as usize;
    let optlen_ptr = optlen as *mut socklen_t as usize;
    syscall5(
        SYS_GETSOCKOPT,
        sockfd,
        level,
        optname,
        optval_ptr,
        optlen_ptr,
    )
    .map(drop)
}

/// Get options on sockets
pub unsafe fn getsockopt2(
    sockfd: i32,
    level: i32,
    optname: i32,
    optval: &mut usize,
    optlen: &mut socklen_t,
) -> Result<(), Errno> {
    let sockfd = sockfd as usize;
    let level = level as usize;
    let optname = optname as usize;
    let optval_ptr = optval as *mut usize as usize;
    let optlen_ptr = optlen as *mut socklen_t as usize;
    syscall5(
        SYS_GETSOCKOPT2,
        sockfd,
        level,
        optname,
        optval_ptr,
        optlen_ptr,
    )
    .map(drop)
}

/// Get the real user ID of the calling process.
///
/// # Example
///
/// ```
/// let uid = unsafe { nc::getuid() };
/// assert!(uid > 0);
/// ```
#[must_use]
pub unsafe fn getuid() -> uid_t {
    // This function is always successful.
    syscall0(SYS_GETUID).expect("getuid() failed") as uid_t
}

/// Get list of all mounted file systems.
///
/// If buf is None, returns number of mounted file systems.
pub unsafe fn getvfsstat(buf: Option<&mut [statvfs_t]>, mode: i32) -> Result<i32, Errno> {
    let buf_size = buf
        .as_ref()
        .map_or(0, |buf| buf.len() * core::mem::size_of::<statvfs_t>());
    let buf_ptr = buf.map_or(0, |buf| buf.as_mut_ptr() as usize);
    let mode = mode as usize;
    syscall3(SYS_GETVFSSTAT, buf_ptr, buf_size, mode).map(|val| val as i32)
}

/// Get extended attribute value.
///
/// # Example
///
/// ```
/// let path = "/tmp/nc-getxattr";
/// let ret = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_WRONLY | nc::O_CREAT, 0o644) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// let attr_name = "user.creator";
/// let attr_value = "nc-0.0.1";
/// //let flags = 0;
/// let flags = nc::XATTR_CREATE;
/// let ret = unsafe {
///     nc::setxattr(
///         path,
///         &attr_name,
///         attr_value.as_ptr() as usize,
///         attr_value.len(),
///         flags,
///     )
/// };
/// assert!(ret.is_ok());
/// let mut buf = [0_u8; 16];
/// let buf_len = buf.len();
/// let ret = unsafe { nc::getxattr(path, attr_name, buf.as_mut_ptr() as usize, buf_len) };
/// assert!(ret.is_ok());
/// assert_eq!(ret, Ok(attr_value.len() as nc::ssize_t));
/// let attr_len = ret.unwrap() as usize;
/// assert_eq!(attr_value.as_bytes(), &buf[..attr_len]);
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, path, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn getxattr<P: AsRef<Path>>(
    filename: P,
    name: P,
    value: usize,
    size: size_t,
) -> Result<ssize_t, Errno> {
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    let name = CString::new(name.as_ref());
    let name_ptr = name.as_ptr() as usize;
    syscall4(SYS_GETXATTR, filename_ptr, name_ptr, value, size).map(|ret| ret as ssize_t)
}

/// Control device.
///
/// # Example
///
/// ```
/// let path = "/tmp/nc-ioctl";
/// let ret = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_WRONLY | nc::O_CREAT, 0o644) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
/// let mut attr: i32 = 0;
/// let cmd = -2146933247; // nc::FS_IOC_GETFLAGS
/// let ret = unsafe { nc::ioctl(fd, cmd, &mut attr as *mut i32 as usize) };
/// assert!(ret.is_ok());
/// println!("attr: {}", attr);
///
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, path, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn ioctl(fd: i32, cmd: i32, arg: usize) -> Result<(), Errno> {
    let fd = fd as usize;
    let cmd = cmd as usize;
    syscall3(SYS_IOCTL, fd, cmd, arg).map(drop)
}

/// Is current process tainted by uid or gid changes
///
/// Returns 1 if the process environment or memory address space is considered “tainted”,
/// and returns 0 otherwise.
#[must_use]
pub unsafe fn issetugid() -> i32 {
    // This function is always successful.
    syscall0(SYS_ISSETUGID)
        .map(|val| val as i32)
        .expect("issetugid() failed")
}

/// Send signal to a process.
///
/// # Example
///
/// ```
/// let pid = unsafe { nc::fork() };
/// assert!(pid.is_ok());
/// let pid = pid.unwrap();
/// assert!(pid >= 0);
/// if pid == 0 {
///     // child process.
///     let args = [""];
///     let env = [""];
///     let ret = unsafe { nc::execve("/usr/bin/yes", &args, &env) };
///     assert!(ret.is_ok());
/// } else {
///     // parent process.
///     let ret = unsafe { nc::kill(pid, nc::SIGTERM) };
///     assert!(ret.is_ok());
/// }
/// ```
pub unsafe fn kill(pid: pid_t, signal: i32) -> Result<(), Errno> {
    let pid = pid as usize;
    let signal = signal as usize;
    syscall2(SYS_KILL, pid, signal).map(drop)
}

/// Creates a new kernel event queue and returns a descriptor.
pub unsafe fn kqueue() -> Result<i32, Errno> {
    syscall0(SYS_KQUEUE).map(|val| val as i32)
}

/// Creates a new kernel event queue and returns a descriptor.
pub unsafe fn kqueue1(flags: i32) -> Result<i32, Errno> {
    let flags = flags as usize;
    syscall1(SYS_KQUEUE1, flags).map(|val| val as i32)
}

/// Enables or disables tracing of one or more processes.
pub unsafe fn ktrace<P: AsRef<Path>>(
    tracefile: P,
    ops: i32,
    facs: i32,
    pid: i32,
) -> Result<(), Errno> {
    let tracefile = CString::new(tracefile.as_ref());
    let tracefile_ptr = tracefile.as_ptr() as usize;
    let ops = ops as usize;
    let facs = facs as usize;
    let pid = pid as usize;
    syscall4(SYS_KTRACE, tracefile_ptr, ops, facs, pid).map(drop)
}

/// Set file flags.
pub unsafe fn lchflags<P: AsRef<Path>>(path: P, flags: fflags_t) -> Result<(), Errno> {
    let path = CString::new(path.as_ref());
    let path_ptr = path.as_ptr() as usize;
    let flags = flags as usize;
    syscall2(SYS_LCHFLAGS, path_ptr, flags).map(drop)
}

/// Change permissions of a file.
pub unsafe fn lchmod<P: AsRef<Path>>(filename: P, mode: mode_t) -> Result<(), Errno> {
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    let mode = mode as usize;
    syscall2(SYS_LCHMOD, filename_ptr, mode).map(drop)
}

/// Change ownership of a file. Does not deference symbolic link.
///
/// # Example
///
/// ```
/// let filename = "/tmp/nc-lchown";
/// let ret = unsafe {
///     nc::openat(
///         nc::AT_FDCWD,
///         filename,
///         nc::O_CREAT | nc::O_WRONLY | nc::O_TRUNC,
///         0o644
///     )
/// };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::lchown(filename, 0, 0) };
/// assert!(ret.is_err());
/// assert_eq!(ret, Err(nc::EPERM));
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, filename, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn lchown<P: AsRef<Path>>(filename: P, user: uid_t, group: gid_t) -> Result<(), Errno> {
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    let user = user as usize;
    let group = group as usize;
    syscall3(SYS_LCHOWN, filename_ptr, user, group).map(drop)
}

/// Get extended attribute value.
///
/// # Example
///
/// ```
/// let path = "/tmp/nc-lgetxattr";
/// let ret = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_WRONLY | nc::O_CREAT, 0o644) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// let attr_name = "user.creator";
/// let attr_value = "nc-0.0.1";
/// //let flags = 0;
/// let flags = nc::XATTR_CREATE;
/// let ret = unsafe {
///     nc::setxattr(
///         path,
///         &attr_name,
///         attr_value.as_ptr() as usize,
///         attr_value.len(),
///         flags,
///     )
/// };
/// assert!(ret.is_ok());
/// let mut buf = [0_u8; 16];
/// let buf_len = buf.len();
/// let ret = unsafe { nc::lgetxattr(path, attr_name, buf.as_mut_ptr() as usize, buf_len) };
/// assert!(ret.is_ok());
/// assert_eq!(ret, Ok(attr_value.len() as nc::ssize_t));
/// let attr_len = ret.unwrap() as usize;
/// assert_eq!(attr_value.as_bytes(), &buf[..attr_len]);
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, path, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn lgetxattr<P: AsRef<Path>>(
    filename: P,
    name: P,
    value: usize,
    size: size_t,
) -> Result<ssize_t, Errno> {
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    let name = CString::new(name.as_ref());
    let name_ptr = name.as_ptr() as usize;
    syscall4(SYS_LGETXATTR, filename_ptr, name_ptr, value, size).map(|ret| ret as ssize_t)
}

/// Make a new name for a file.
///
/// # Example
///
/// ```
/// let old_filename = "/tmp/nc-link-src";
/// let ret = unsafe {
///     nc::openat(
///         nc::AT_FDCWD,
///         old_filename,
///         nc::O_CREAT | nc::O_WRONLY | nc::O_TRUNC,
///         0o644
///     )
/// };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// let new_filename = "/tmp/nc-link-dst";
/// let ret = unsafe { nc::link(old_filename, new_filename) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, old_filename, 0) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, new_filename, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn link<P: AsRef<Path>>(old_filename: P, new_filename: P) -> Result<(), Errno> {
    let old_filename = CString::new(old_filename.as_ref());
    let old_filename_ptr = old_filename.as_ptr() as usize;
    let new_filename = CString::new(new_filename.as_ref());
    let new_filename_ptr = new_filename.as_ptr() as usize;
    syscall2(SYS_LINK, old_filename_ptr, new_filename_ptr).map(drop)
}

/// Make a new name for a file.
///
/// # Example
///
/// ```
/// let old_filename = "/tmp/nc-linkat-src";
/// let ret = unsafe { nc::openat(nc::AT_FDCWD, old_filename, nc::O_WRONLY | nc::O_CREAT, 0o644) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// let new_filename = "/tmp/nc-linkat-dst";
/// let flags = nc::AT_SYMLINK_FOLLOW;
/// let ret = unsafe { nc::linkat(nc::AT_FDCWD, old_filename, nc::AT_FDCWD,  new_filename, flags) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, old_filename, 0) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, new_filename, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn linkat<P: AsRef<Path>>(
    olddfd: i32,
    oldfilename: P,
    newdfd: i32,
    newfilename: P,
    flags: i32,
) -> Result<(), Errno> {
    let olddfd = olddfd as usize;
    let oldfilename = CString::new(oldfilename.as_ref());
    let oldfilename_ptr = oldfilename.as_ptr() as usize;
    let newdfd = newdfd as usize;
    let newfilename = CString::new(newfilename.as_ref());
    let newfilename_ptr = newfilename.as_ptr() as usize;
    let flags = flags as usize;
    syscall5(
        SYS_LINKAT,
        olddfd,
        oldfilename_ptr,
        newdfd,
        newfilename_ptr,
        flags,
    )
    .map(drop)
}

/// List directed I/O (REALTIME)
pub unsafe fn lio_listio(
    mode: i32,
    acb_list: &mut [aiocb_t],
    sig: &mut sigevent_t,
) -> Result<(), Errno> {
    let mode = mode as usize;
    let acb_list_ptr = acb_list.as_mut_ptr() as usize;
    let acb_list_len = acb_list.len();
    let sig_ptr = sig as *mut sigevent_t as usize;
    syscall4(SYS_LIO_LISTIO, mode, acb_list_ptr, acb_list_len, sig_ptr).map(drop)
}

/// Listen for connections on a socket.
pub unsafe fn listen(sockfd: i32, backlog: i32) -> Result<(), Errno> {
    let sockfd = sockfd as usize;
    let backlog = backlog as usize;
    syscall2(SYS_LISTEN, sockfd, backlog).map(drop)
}

/// List extended attribute names.
///
/// # Example
///
/// ```
/// let path = "/tmp/nc-listxattr";
/// let ret = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_WRONLY | nc::O_CREAT, 0o644) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// let attr_name = "user.creator";
/// let attr_value = "nc-0.0.1";
/// //let flags = 0;
/// let flags = nc::XATTR_CREATE;
/// let ret = unsafe {
///     nc::setxattr(
///         path,
///         &attr_name,
///         attr_value.as_ptr() as usize,
///         attr_value.len(),
///         flags,
///     )
/// };
/// assert!(ret.is_ok());
/// let mut buf = [0_u8; 16];
/// let buf_len = buf.len();
/// let ret = unsafe { nc::listxattr(path, buf.as_mut_ptr() as usize, buf_len) };
/// let attr_len = ret.unwrap() as usize;
/// assert_eq!(&buf[..attr_len - 1], attr_name.as_bytes());
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, path, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn listxattr<P: AsRef<Path>>(
    filename: P,
    list: usize,
    size: size_t,
) -> Result<ssize_t, Errno> {
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    syscall3(SYS_LISTXATTR, filename_ptr, list, size).map(|ret| ret as ssize_t)
}

/// List extended attribute names.
///
/// # Example
///
/// ```
/// let path = "/tmp/nc-llistxattr";
/// let ret = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_WRONLY | nc::O_CREAT, 0o644) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// let attr_name = "user.creator";
/// let attr_value = "nc-0.0.1";
/// //let flags = 0;
/// let flags = nc::XATTR_CREATE;
/// let ret = unsafe {
///     nc::setxattr(
///         path,
///         &attr_name,
///         attr_value.as_ptr() as usize,
///         attr_value.len(),
///         flags,
///     )
/// };
/// assert!(ret.is_ok());
/// let mut buf = [0_u8; 16];
/// let buf_len = buf.len();
/// let ret = unsafe { nc::llistxattr(path, buf.as_mut_ptr() as usize, buf_len) };
/// let attr_len = ret.unwrap() as usize;
/// assert_eq!(&buf[..attr_len - 1], attr_name.as_bytes());
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, path, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn llistxattr<P: AsRef<Path>>(
    filename: P,
    list: usize,
    size: size_t,
) -> Result<ssize_t, Errno> {
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    syscall3(SYS_LLISTXATTR, filename_ptr, list, size).map(|ret| ret as ssize_t)
}

/// Remove an extended attribute.
///
/// # Example
///
/// ```
/// let path = "/tmp/nc-lremovexattr";
/// let ret = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_WRONLY | nc::O_CREAT, 0o644) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// let attr_name = "user.creator";
/// let attr_value = "nc-0.0.1";
/// let flags = nc::XATTR_CREATE;
/// let ret = unsafe {
///     nc::setxattr(
///         path,
///         &attr_name,
///         attr_value.as_ptr() as usize,
///         attr_value.len(),
///         flags,
///     )
/// };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::lremovexattr(path, attr_name) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, path, 0) };
/// ```
pub unsafe fn lremovexattr<P: AsRef<Path>>(filename: P, name: P) -> Result<(), Errno> {
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    let name = CString::new(name.as_ref());
    let name_ptr = name.as_ptr() as usize;
    syscall2(SYS_LREMOVEXATTR, filename_ptr, name_ptr).map(drop)
}

/// Reposition file offset.
///
/// # Example
///
/// ```
/// let path = "/etc/passwd";
/// let ret = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_RDONLY, 0) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
/// let ret = unsafe { nc::lseek(fd, 42, nc::SEEK_SET) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn lseek(fd: i32, offset: off_t, whence: i32) -> Result<(), Errno> {
    let fd = fd as usize;
    let offset = offset as usize;
    let whence = whence as usize;
    syscall3(SYS_LSEEK, fd, offset, whence).map(drop)
}

/// Set extended attribute value.
///
/// # Example
///
/// ```
/// let path = "/tmp/nc-lsetxattr";
/// let ret = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_WRONLY | nc::O_CREAT, 0o644) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// let attr_name = "user.creator";
/// let attr_value = "nc-0.0.1";
/// //let flags = 0;
/// let flags = nc::XATTR_CREATE;
/// let ret = unsafe {
///     nc::lsetxattr(
///         path,
///         &attr_name,
///         attr_value.as_ptr() as usize,
///         attr_value.len(),
///         flags,
///     )
/// };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, path, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn lsetxattr<P: AsRef<Path>>(
    filename: P,
    name: P,
    value: usize,
    size: size_t,
    flags: i32,
) -> Result<(), Errno> {
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    let name = CString::new(name.as_ref());
    let name_ptr = name.as_ptr() as usize;
    let flags = flags as usize;
    syscall5(SYS_LSETXATTR, filename_ptr, name_ptr, value, size, flags).map(drop)
}

/// Give advice about use of memory.
///
/// # Example
///
/// ```
/// // Initialize an anonymous mapping with 4 pages.
/// let map_length = 4 * nc::PAGE_SIZE;
/// let addr = unsafe {
///     nc::mmap(
///         0,
///         map_length,
///         nc::PROT_READ | nc::PROT_WRITE,
///         nc::MAP_PRIVATE | nc::MAP_ANONYMOUS,
///         -1,
///         0,
///     )
/// };
/// assert!(addr.is_ok());
/// let addr = addr.unwrap();
///
/// // Set the third page readonly. And we will run into SIGSEGV when updating it.
/// let ret = unsafe { nc::madvise(addr + 2 * nc::PAGE_SIZE, nc::PAGE_SIZE, nc::MADV_RANDOM) };
/// assert!(ret.is_ok());
///
/// let ret = unsafe { nc::munmap(addr, map_length) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn madvise(addr: usize, len: size_t, advice: i32) -> Result<(), Errno> {
    let advice = advice as usize;
    syscall3(SYS_MADVISE, addr, len, advice).map(drop)
}

/// mincore() returns the memory residency status of the pages in the
/// current process's address space specified by [addr, addr + len).
/// The status is returned in a vector of bytes.  The least significant
/// bit of each byte is 1 if the referenced page is in memory, otherwise
/// it is zero.
///
/// Because the status of a page can change after mincore() checks it
/// but before it returns to the application, the returned vector may
/// contain stale information.  Only locked pages are guaranteed to
/// remain in memory.
///
/// return values:
///  zero    - success
///  -EFAULT - vec points to an illegal address
///  -EINVAL - addr is not a multiple of `PAGE_SIZE`
///  -ENOMEM - Addresses in the range [addr, addr + len] are
/// invalid for the address space of this process, or specify one or
/// more pages which are not currently mapped
///  -EAGAIN - A kernel resource was temporarily unavailable.
pub unsafe fn mincore(start: usize, len: size_t, vec: *const u8) -> Result<(), Errno> {
    let vec_ptr = vec as usize;
    syscall3(SYS_MINCORE, start, len, vec_ptr).map(drop)
}

/// Control the inheritance of pages
pub unsafe fn minherit(addr: usize, len: size_t, inherit: i32) -> Result<(), Errno> {
    let inherit = inherit as usize;
    syscall3(SYS_MINHERIT, addr, len, inherit).map(drop)
}

/// Create a directory.
///
/// # Example
///
/// ```
/// let path = "/tmp/nc-mkdir";
/// let ret = unsafe { nc::mkdir(path, 0o755) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, path, nc::AT_REMOVEDIR) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn mkdir<P: AsRef<Path>>(filename: P, mode: mode_t) -> Result<(), Errno> {
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    let mode = mode as usize;
    syscall2(SYS_MKDIR, filename_ptr, mode).map(drop)
}

/// Create a directory.
///
/// # Example
///
/// ```
/// let path = "/tmp/nc-mkdir";
/// let ret = unsafe { nc::mkdirat(nc::AT_FDCWD, path, 0o755) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, path, nc::AT_REMOVEDIR) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn mkdirat<P: AsRef<Path>>(dirfd: i32, filename: P, mode: mode_t) -> Result<(), Errno> {
    let dirfd = dirfd as usize;
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    let mode = mode as usize;
    syscall3(SYS_MKDIRAT, dirfd, filename_ptr, mode).map(drop)
}

/// Create a fifo file.
pub unsafe fn mkfifo<P: AsRef<Path>>(path: P, mode: mode_t) -> Result<(), Errno> {
    let path = CString::new(path.as_ref());
    let path_ptr = path.as_ptr() as usize;
    let mode = mode as usize;
    syscall2(SYS_MKFIFO, path_ptr, mode).map(drop)
}

/// Create a fifo file.
pub unsafe fn mkfifoat<P: AsRef<Path>>(dfd: i32, path: P, mode: mode_t) -> Result<(), Errno> {
    let dfd = dfd as usize;
    let path = CString::new(path.as_ref());
    let path_ptr = path.as_ptr() as usize;
    let mode = mode as usize;
    syscall3(SYS_MKFIFOAT, dfd, path_ptr, mode).map(drop)
}

/// Create a special or ordinary file.
///
/// # Example
///
/// ```
/// let path = "/tmp/nc-mknodat";
/// // Create a named pipe.
/// let ret = unsafe { nc::mknodat(nc::AT_FDCWD, path, nc::S_IFIFO | nc::S_IRUSR | nc::S_IWUSR, 0) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, path, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn mknodat<P: AsRef<Path>>(
    dirfd: i32,
    filename: P,
    mode: mode_t,
    dev: dev_t,
) -> Result<(), Errno> {
    let dirfd = dirfd as usize;
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    let mode = mode as usize;
    let dev = dev as usize;
    syscall4(SYS_MKNODAT, dirfd, filename_ptr, mode, dev).map(drop)
}

/// Lock memory.
///
/// # Example
///
/// ```
/// let mut passwd_buf = [0_u8; 64];
/// let ret = unsafe { nc::mlock(passwd_buf.as_ptr() as usize, passwd_buf.len()) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn mlock(addr: usize, len: size_t) -> Result<(), Errno> {
    syscall2(SYS_MLOCK, addr, len).map(drop)
}

/// Lock memory.
///
/// # Example
///
/// ```
/// let ret = unsafe { nc::mlockall(nc::MCL_CURRENT) };
/// // We got out-of-memory error in CI environment.
/// assert!(ret.is_ok() || ret == Err(nc::ENOMEM));
/// ```
pub unsafe fn mlockall(flags: i32) -> Result<(), Errno> {
    let flags = flags as usize;
    syscall1(SYS_MLOCKALL, flags).map(drop)
}

/// Map files or devices into memory.
///
/// # Example
///
/// ```
/// let path = "/etc/passwd";
/// let ret = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_RDONLY, 0o644) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
///
/// let mut sb = nc::stat_t::default();
/// let ret = unsafe { nc::fstat(fd, &mut sb) };
/// assert!(ret.is_ok());
///
/// let offset: usize = 0;
/// let length: usize = sb.st_size as usize - offset;
/// // Offset for mmap must be page aligned.
/// let pa_offset: usize = offset & !(nc::PAGE_SIZE - 1);
/// let map_length = length + offset - pa_offset;
///
/// let addr = unsafe {
///     nc::mmap(
///         0, // 0 as NULL
///         map_length,
///         nc::PROT_READ,
///         nc::MAP_PRIVATE,
///         fd,
///         pa_offset as nc::off_t,
///     )
/// };
/// assert!(addr.is_ok());
/// let addr = addr.unwrap();
///
/// let n_write = unsafe { nc::write(1, addr + offset - pa_offset, length) };
/// assert!(n_write.is_ok());
/// assert_eq!(n_write, Ok(length as nc::ssize_t));
/// let ret = unsafe { nc::munmap(addr, map_length) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn mmap(
    start: usize,
    len: size_t,
    prot: i32,
    flags: i32,
    fd: i32,
    offset: off_t,
) -> Result<usize, Errno> {
    let prot = prot as usize;
    let flags = flags as usize;
    let fd = fd as usize;
    let offset = offset as usize;
    syscall6(SYS_MMAP, start, len, prot, flags, fd, offset)
}

/// Module control.
pub unsafe fn modctl(operation: i32, arg: uintptr_t) -> Result<(), Errno> {
    let operation = operation as usize;
    syscall2(SYS_MODCTL, operation, arg).map(drop)
}

/// Set protection on a region of memory.
///
/// # Example
///
/// ```
/// // Initialize an anonymous mapping with 4 pages.
/// let map_length = 4 * nc::PAGE_SIZE;
/// let addr = unsafe {
///     nc::mmap(
///         0,
///         map_length,
///         nc::PROT_READ | nc::PROT_WRITE,
///         nc::MAP_PRIVATE | nc::MAP_ANONYMOUS,
///         -1,
///         0,
///     )
/// };
/// assert!(addr.is_ok());
/// let addr = addr.unwrap();
///
/// // Set the third page readonly. And we will run into SIGSEGV when updating it.
/// let ret = unsafe { nc::mprotect(addr + 2 * nc::PAGE_SIZE, nc::PAGE_SIZE, nc::PROT_READ) };
/// assert!(ret.is_ok());
///
/// let ret = unsafe { nc::munmap(addr, map_length) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn mprotect(addr: usize, len: size_t, prot: i32) -> Result<(), Errno> {
    let prot = prot as usize;
    syscall3(SYS_MPROTECT, addr, len, prot).map(drop)
}

/// Close a message queue (realtime)
pub unsafe fn mq_close(mqdes: mqd_t) -> Result<(), Errno> {
    let mqdes = mqdes as usize;
    syscall1(SYS_MQ_CLOSE, mqdes).map(drop)
}

/// Get message queue attributes (realtime)
pub unsafe fn mq_getattr(mqdes: mqd_t, attr: Option<&mut mq_attr_t>) -> Result<(), Errno> {
    let mqdes = mqdes as usize;
    let attr_ptr = attr.map_or(0, |attr| attr as *mut mq_attr_t as usize);
    syscall2(SYS_MQ_GETATTR, mqdes, attr_ptr).map(drop)
}

/// Register for notification when a message is available
pub unsafe fn mq_notify(mqdes: mqd_t, notification: Option<&sigevent_t>) -> Result<(), Errno> {
    let mqdes = mqdes as usize;
    let notification_ptr =
        notification.map_or(0, |notification| notification as *const sigevent_t as usize);
    syscall2(SYS_MQ_NOTIFY, mqdes, notification_ptr).map(drop)
}

/// Open a message queue.
///
/// # Example
///
/// ```
/// let name = "nc-posix-mq";
/// let ret = unsafe {
///     nc::mq_open(
///         name,
///         nc::O_CREAT | nc::O_RDWR,
///         (nc::S_IRUSR | nc::S_IWUSR) as nc::mode_t,
///         None,
///     )
/// };
/// assert!(ret.is_ok());
/// let mq_id = ret.unwrap();
/// let ret = unsafe { nc::close(mq_id) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::mq_unlink(name) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn mq_open<P: AsRef<Path>>(
    name: P,
    oflag: i32,
    mode: mode_t,
    attr: Option<&mut mq_attr_t>,
) -> Result<mqd_t, Errno> {
    let name = CString::new(name.as_ref());
    let name_ptr = name.as_ptr() as usize;
    let oflag = oflag as usize;
    let mode = mode as usize;
    let attr_ptr = attr.map_or(0, |attr| attr as *mut mq_attr_t as usize);
    syscall4(SYS_MQ_OPEN, name_ptr, oflag, mode, attr_ptr).map(|ret| ret as mqd_t)
}

/// Receive a message from a message queue (realtime)
pub unsafe fn mq_receive(
    mqdes: mqd_t,
    msg: &mut [u8],
    msg_len: usize,
    msg_prio: &mut u32,
) -> Result<ssize_t, Errno> {
    let mqdes = mqdes as usize;
    let msg = CString::new(msg);
    let msg_ptr = msg.as_ptr() as usize;
    let msg_prio = msg_prio as *mut u32 as usize;
    syscall4(SYS_MQ_RECEIVE, mqdes, msg_ptr, msg_len, msg_prio).map(|ret| ret as ssize_t)
}

/// Send message to a message queue (realtime)
pub unsafe fn mq_send(
    mqdes: mqd_t,
    msg: &[u8],
    msg_len: usize,
    msg_prio: u32,
) -> Result<(), Errno> {
    let mqdes = mqdes as usize;
    let msg = CString::new(msg);
    let msg_ptr = msg.as_ptr() as usize;
    let msg_prio = msg_prio as usize;
    syscall4(SYS_MQ_SEND, mqdes, msg_ptr, msg_len, msg_prio).map(drop)
}

/// Set message queue attributes (realtime)
pub unsafe fn mq_setattr(
    mqdes: mqd_t,
    new_attr: Option<&mut mq_attr_t>,
    old_attr: Option<&mut mq_attr_t>,
) -> Result<(), Errno> {
    let mqdes = mqdes as usize;
    let new_attr_ptr = new_attr.map_or(0, |new_attr| new_attr as *mut mq_attr_t as usize);
    let old_attr_ptr = old_attr.map_or(0, |old_attr| old_attr as *mut mq_attr_t as usize);
    syscall3(SYS_MQ_SETATTR, mqdes, new_attr_ptr, old_attr_ptr).map(drop)
}

/// Remove a message queue.
///
/// # Example
///
/// ```
/// let name = "nc-mq-unlink";
/// let ret = unsafe {
///     nc::mq_open(
///         name,
///         nc::O_CREAT | nc::O_RDWR,
///         (nc::S_IRUSR | nc::S_IWUSR) as nc::umode_t,
///         None,
///     )
/// };
/// assert!(ret.is_ok());
/// let mq_id = ret.unwrap();
/// let ret = unsafe { nc::close(mq_id) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::mq_unlink(name) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn mq_unlink<P: AsRef<Path>>(name: P) -> Result<(), Errno> {
    let name = CString::new(name.as_ref());
    let name_ptr = name.as_ptr() as usize;
    syscall1(SYS_MQ_UNLINK, name_ptr).map(drop)
}

/// Remap a virtual memory address
pub unsafe fn mremap(
    addr: usize,
    old_len: size_t,
    new_len: size_t,
    flags: usize,
    new_addr: usize,
) -> Result<usize, Errno> {
    syscall5(SYS_MREMAP, addr, old_len, new_len, flags, new_addr)
}

/// Get a System V message queue identifier.
///
/// # Example
///
/// ```
/// let key = nc::IPC_PRIVATE;
/// let flags = nc::IPC_CREAT | nc::IPC_EXCL | (nc::S_IRUSR | nc::S_IWUSR) as i32;
/// let ret = unsafe { nc::msgget(key, flags) };
/// assert!(ret.is_ok());
/// let msq_id = ret.unwrap();

/// let mut buf = nc::msqid_ds_t::default();
/// let ret = unsafe { nc::msgctl(msq_id, nc::IPC_RMID, &mut buf) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn msgget(key: key_t, msgflg: i32) -> Result<i32, Errno> {
    let key = key as usize;
    let msgflg = msgflg as usize;
    syscall2(SYS_MSGGET, key, msgflg).map(|ret| ret as i32)
}

/// Receive messages from a System V message queue.
///
/// # Example
///
/// ```
/// const MAX_MTEXT: usize = 1024;
///
/// const MTYPE_NULL: isize = 0;
/// const MTYPE_CLIENT: isize = 1;
/// const _MTYPE_SERVER: isize = 2;
///
/// #[derive(Debug, Clone, Copy)]
/// #[repr(C)]
/// struct Message {
///     pub mtype: isize,
///     pub mtext: [u8; MAX_MTEXT],
/// }
///
/// impl Default for Message {
///     fn default() -> Self {
///         Message {
///             mtype: MTYPE_NULL,
///             mtext: [0; MAX_MTEXT],
///         }
///     }
/// }
///
/// fn main() {
///     let key = nc::IPC_PRIVATE;
///     let flags = nc::IPC_CREAT | nc::IPC_EXCL | (nc::S_IRUSR | nc::S_IWUSR) as i32;
///     let ret = unsafe { nc::msgget(key, flags) };
///     assert!(ret.is_ok());
///     let msq_id = ret.unwrap();
///
///     // Write to message queue.
///     let msg = "Hello, Rust";
///     let mut client_msg = Message {
///         mtype: MTYPE_CLIENT,
///         mtext: [0; MAX_MTEXT],
///     };
///     let msg_len = msg.len();
///     unsafe {
///         let src_ptr = msg.as_ptr();
///         let dst_ptr = client_msg.mtext.as_mut_ptr();
///         core::ptr::copy_nonoverlapping(src_ptr, dst_ptr, msg_len);
///     }
///
///     let ret = unsafe { nc::msgsnd(msq_id, &client_msg as *const Message as usize, msg_len, 0) };
///     assert!(ret.is_ok());
///
///     // Read from message queue.
///     let mut recv_msg = Message::default();
///     let ret = unsafe {
///         nc::msgrcv(
///             msq_id,
///             &mut recv_msg as *mut Message as usize,
///             MAX_MTEXT,
///             MTYPE_CLIENT,
///             0,
///         )
///     };
///     assert!(ret.is_ok());
///     let recv_msg_len = ret.unwrap() as usize;
///     assert_eq!(recv_msg_len, msg_len);
///     let recv_text = core::str::from_utf8(&recv_msg.mtext[..recv_msg_len]);
///     assert!(recv_text.is_ok());
///     let recv_text = recv_text.unwrap();
///     assert_eq!(recv_text, msg);
///     println!("recv text: {}", recv_text);
///
///     let mut buf = nc::msqid_ds_t::default();
///     let ret = unsafe { nc::msgctl(msq_id, nc::IPC_RMID, &mut buf) };
///     assert!(ret.is_ok());
/// }
/// ```
pub unsafe fn msgrcv(
    msqid: i32,
    msgq: usize,
    msgsz: size_t,
    msgtyp: isize,
    msgflg: i32,
) -> Result<ssize_t, Errno> {
    let msqid = msqid as usize;
    let msgtyp = msgtyp as usize;
    let msgflg = msgflg as usize;
    syscall5(SYS_MSGRCV, msqid, msgq, msgsz, msgtyp, msgflg).map(|ret| ret as ssize_t)
}

/// Append the message to a System V message queue.
///
/// # Example
///
/// ```
/// const MAX_MTEXT: usize = 1024;
///
/// const MTYPE_NULL: isize = 0;
/// const MTYPE_CLIENT: isize = 1;
/// const _MTYPE_SERVER: isize = 2;
///
/// #[derive(Debug, Clone, Copy)]
/// #[repr(C)]
/// struct Message {
///     pub mtype: isize,
///     pub mtext: [u8; MAX_MTEXT],
/// }
///
/// impl Default for Message {
///     fn default() -> Self {
///         Message {
///             mtype: MTYPE_NULL,
///             mtext: [0; MAX_MTEXT],
///         }
///     }
/// }
///
/// fn main() {
///     let key = nc::IPC_PRIVATE;
///     let flags = nc::IPC_CREAT | nc::IPC_EXCL | (nc::S_IRUSR | nc::S_IWUSR) as i32;
///     let ret = unsafe { nc::msgget(key, flags) };
///     assert!(ret.is_ok());
///     let msq_id = ret.unwrap();
///
///     // Write to message queue.
///     let msg = "Hello, Rust";
///     let mut client_msg = Message {
///         mtype: MTYPE_CLIENT,
///         mtext: [0; MAX_MTEXT],
///     };
///     let msg_len = msg.len();
///     unsafe {
///         let src_ptr = msg.as_ptr();
///         let dst_ptr = client_msg.mtext.as_mut_ptr();
///         core::ptr::copy_nonoverlapping(src_ptr, dst_ptr, msg_len);
///     }
///
///     let ret = unsafe { nc::msgsnd(msq_id, &client_msg as *const Message as usize, msg_len, 0) };
///     assert!(ret.is_ok());
///
///     // Read from message queue.
///     let mut recv_msg = Message::default();
///     let ret = unsafe {
///         nc::msgrcv(
///             msq_id,
///             &mut recv_msg as *mut Message as usize,
///             MAX_MTEXT,
///             MTYPE_CLIENT,
///             0,
///         )
///     };
///     assert!(ret.is_ok());
///     let recv_msg_len = ret.unwrap() as usize;
///     assert_eq!(recv_msg_len, msg_len);
///     let recv_text = core::str::from_utf8(&recv_msg.mtext[..recv_msg_len]);
///     assert!(recv_text.is_ok());
///     let recv_text = recv_text.unwrap();
///     assert_eq!(recv_text, msg);
///
///     let mut buf = nc::msqid_ds_t::default();
///     let ret = unsafe { nc::msgctl(msq_id, nc::IPC_RMID, &mut buf) };
///     assert!(ret.is_ok());
/// }
/// ```
pub unsafe fn msgsnd(msqid: i32, msgq: usize, msgsz: size_t, msgflg: i32) -> Result<(), Errno> {
    let msqid = msqid as usize;
    let msgflg = msgflg as usize;
    syscall4(SYS_MSGSND, msqid, msgq, msgsz, msgflg).map(drop)
}

/// Unlock memory.
///
/// # Example
///
/// ```
/// let mut passwd_buf = [0_u8; 64];
/// let addr = passwd_buf.as_ptr() as usize;
/// let ret = unsafe { nc::mlock2(addr, passwd_buf.len(), nc::MCL_CURRENT) };
/// for i in 0..passwd_buf.len() {
///   passwd_buf[i] = i as u8;
/// }
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::munlock(addr, passwd_buf.len()) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn munlock(addr: usize, len: size_t) -> Result<(), Errno> {
    syscall2(SYS_MUNLOCK, addr, len).map(drop)
}

/// Unlock memory.
///
/// # Example
///
/// ```
/// let ret = unsafe { nc::mlockall(nc::MCL_CURRENT) };
/// assert!(ret.is_ok() || ret == Err(nc::ENOMEM));
/// let ret = unsafe { nc::munlockall() };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn munlockall() -> Result<(), Errno> {
    syscall0(SYS_MUNLOCKALL).map(drop)
}

/// Unmap files or devices from memory.
///
/// # Example
///
/// ```
/// let path = "/etc/passwd";
/// let ret = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_RDONLY, 0o644) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
///
/// let mut sb = nc::stat_t::default();
/// let ret = unsafe { nc::fstat(fd, &mut sb) };
/// assert!(ret.is_ok());
///
/// let offset: usize = 0;
/// let length: usize = sb.st_size as usize - offset;
/// // Offset for mmap must be page aligned.
/// let pa_offset: usize = offset & !(nc::PAGE_SIZE - 1);
/// let map_length = length + offset - pa_offset;
///
/// let addr = unsafe {
///     nc::mmap(
///         0, // 0 as NULL
///         map_length,
///         nc::PROT_READ,
///         nc::MAP_PRIVATE,
///         fd,
///         pa_offset as nc::off_t,
///     )
/// };
/// assert!(addr.is_ok());
/// let addr = addr.unwrap();
///
/// let n_write = unsafe { nc::write(1, addr + offset - pa_offset, length) };
/// assert!(n_write.is_ok());
/// assert_eq!(n_write, Ok(length as nc::ssize_t));
/// let ret = unsafe { nc::munmap(addr, map_length) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn munmap(addr: usize, len: size_t) -> Result<(), Errno> {
    syscall2(SYS_MUNMAP, addr, len).map(drop)
}

/// Used by the NFS daemons to pass information into and out of the kernel and
/// also to enter the kernel as a server daemon.
pub unsafe fn nfssvc(fd: i32, args: usize) -> Result<(), Errno> {
    let fd = fd as usize;
    syscall2(SYS_NFSSVC, fd, args).map(drop)
}

/// Used by the NTP daemon to adjust the system clock to an externally derived time.
pub unsafe fn ntp_adjtime(time: &mut timex_t) -> Result<i32, Errno> {
    let time_ptr = time as *mut timex_t as usize;
    syscall1(SYS_NTP_ADJTIME, time_ptr).map(|val| val as i32)
}

/// Open and possibly create a file.
///
/// # Example
///
/// ```
/// let path = "/etc/passwd";
/// let ret = unsafe { nc::open(path, nc::O_RDONLY, 0) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn open<P: AsRef<Path>>(filename: P, flags: i32, mode: mode_t) -> Result<i32, Errno> {
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    let flags = flags as usize;
    let mode = mode as usize;
    syscall3(SYS_OPEN, filename_ptr, flags, mode).map(|ret| ret as i32)
}

/// Open and possibly create a file within a directory.
///
/// # Example
///
/// ```
/// let path = "/etc/passwd";
/// let ret = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_RDONLY, 0) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn openat<P: AsRef<Path>>(
    dirfd: i32,
    filename: P,
    flags: i32,
    mode: mode_t,
) -> Result<i32, Errno> {
    let dirfd = dirfd as usize;
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    let flags = flags as usize;
    let mode = mode as usize;
    syscall4(SYS_OPENAT, dirfd, filename_ptr, flags, mode).map(|ret| ret as i32)
}

/// Accept a connection on a socket.
pub unsafe fn paccept(
    sockfd: i32,
    addr: &mut sockaddr_in_t,
    addrlen: &mut socklen_t,
    sigmask: Option<&sigset_t>,
    flags: i32,
) -> Result<(), Errno> {
    let sockfd = sockfd as usize;
    let addr_ptr = addr as *mut sockaddr_in_t as usize;
    let addrlen_ptr = addrlen as *mut socklen_t as usize;
    let sigmask_ptr = sigmask.map_or(0, |sigmask| sigmask as *const sigset_t as usize);
    let flags = flags as usize;
    syscall5(
        SYS_PACCEPT,
        sockfd,
        addr_ptr,
        addrlen_ptr,
        sigmask_ptr,
        flags,
    )
    .map(drop)
}

/// Get configurable pathname variables
pub unsafe fn pathconf<P: AsRef<Path>>(path: P, name: i32) -> Result<isize, Errno> {
    let path = CString::new(path.as_ref());
    let path_ptr = path.as_ptr() as usize;
    let name = name as usize;
    syscall2(SYS_PATHCONF, path_ptr, name).map(|val| val as isize)
}

/// Create a pipe.
///
/// # Example
///
/// ```
/// let mut fds = [-1_i32, 2];
/// let ret = unsafe { nc::pipe(&mut fds) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::close(fds[0]) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::close(fds[1]) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn pipe(pipefd: &mut [i32; 2]) -> Result<(), Errno> {
    let pipefd_ptr = pipefd.as_mut_ptr() as usize;
    syscall1(SYS_PIPE, pipefd_ptr).map(drop)
}

/// Create a pipe.
///
/// # Example
///
/// ```
/// let mut fds = [-1_i32, 2];
/// let ret = unsafe {nc::pipe2(&mut fds, nc::O_CLOEXEC | nc::O_NONBLOCK) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::close(fds[0]) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::close(fds[1]) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn pipe2(pipefd: &mut [i32; 2], flags: i32) -> Result<(), Errno> {
    let pipefd_ptr = pipefd.as_mut_ptr() as usize;
    let flags = flags as usize;
    syscall2(SYS_PIPE2, pipefd_ptr, flags).map(drop)
}

/// Wait for some event on file descriptors.
///
/// The `timeout` argument specifies the number of milliseconds that `poll()`
/// should block waiting for a file descriptor to become ready. Specifying
/// a timeout of zero causes `poll()` to return immediately, even if
/// no file descriptors are ready.
///
/// ## Return value
/// On success, it returns a nonnegative value which is the number of events
/// in the `fds` whose `revents` fields have been set to a nonzero value.
///
/// # Examples
/// ```rust
/// use std::thread;
/// use std::time::Duration;
///
/// const STDIN_FD: i32 = 0;
/// const STDOUT_FD: i32 = 1;
///
/// fn main() {
///     let mut fds = [
///         nc::pollfd_t {
///             fd: STDIN_FD,
///             events: nc::POLLIN,
///             revents: 0,
///         },
///         nc::pollfd_t {
///             fd: STDOUT_FD,
///             events: nc::POLLOUT,
///             revents: 0,
///         },
///     ];
///
///     // Create a background thread to print some messages to stdout.
///     let handle = thread::spawn(|| {
///         thread::sleep(Duration::from_millis(100));
///         println!("MESSAGES from rust");
///     });
///
///     let ret = unsafe { nc::poll(&mut fds, 3000) };
///     assert!(ret.is_ok());
///     let num_ready = ret.unwrap();
///     println!("num of fds ready to read: {num_ready}");
///     assert!(handle.join().is_ok());
/// }
/// ```
pub unsafe fn poll(fds: &mut [pollfd_t], timeout: i32) -> Result<i32, Errno> {
    let fds_ptr = fds.as_mut_ptr() as usize;
    let nfds = fds.len();
    let timeout = timeout as usize;
    syscall3(SYS_POLL, fds_ptr, nfds, timeout).map(|ret| ret as i32)
}

/// Pre-allocate storage for a range in a file
pub unsafe fn posix_fallocate(fd: i32, offset: off_t, len: off_t) -> Result<(), Errno> {
    let fd = fd as usize;
    let offset = offset as usize;
    let len = len as usize;
    syscall3(SYS_POSIX_FALLOCATE, fd, offset, len).map(drop)
}

/// Read from a file descriptor without changing file offset.
///
/// # Example
///
/// ```
/// let path = "/etc/passwd";
/// let ret = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_RDONLY, 0) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
/// let mut buf = [0_u8; 128];
/// let read_count = 64;
/// let ret = unsafe { nc::pread(fd, buf.as_mut_ptr() as usize, read_count, 0) };
/// assert!(ret.is_ok());
/// assert_eq!(ret, Ok(read_count as nc::ssize_t));
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn pread(fd: i32, buf: usize, count: usize, offset: off_t) -> Result<ssize_t, Errno> {
    let fd = fd as usize;
    let offset = offset as usize;
    syscall4(SYS_PREAD, fd, buf, count, offset).map(|ret| ret as ssize_t)
}

/// Read from a file descriptor without changing file offset.
///
/// # Example
///
/// ```
/// use core::ffi::c_void;
///
/// let path = "/etc/passwd";
/// let ret = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_RDONLY, 0) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
/// let mut buf = [[0_u8; 64]; 4];
/// let capacity = 4 * 64;
/// let mut iov = Vec::with_capacity(buf.len());
/// for ref mut item in (&mut buf).iter() {
///     iov.push(nc::iovec_t {
///         iov_len: item.len(),
///         iov_base: item.as_ptr() as *const c_void,
///     });
/// }
/// let iov_len = iov.len();
/// let ret = unsafe { nc::preadv(fd, &mut iov, 0, iov_len - 1) };
/// assert!(ret.is_ok());
/// assert_eq!(ret, Ok(capacity as nc::ssize_t));
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn preadv(
    fd: i32,
    vec: &mut [iovec_t],
    pos_l: usize,
    pos_h: usize,
) -> Result<ssize_t, Errno> {
    let fd = fd as usize;
    let vec_ptr = vec.as_mut_ptr() as usize;
    let vec_len = vec.len();
    syscall5(SYS_PREADV, fd, vec_ptr, vec_len, pos_l, pos_h).map(|ret| ret as ssize_t)
}

/// Control process profiling
pub unsafe fn profil(samples: &mut [u8], offset: vm_offset_t, scale: i32) -> Result<(), Errno> {
    let samples_ptr = samples.as_mut_ptr() as usize;
    let size = samples.len();
    let scale = scale as usize;
    syscall4(SYS_PROFIL, samples_ptr, size, offset, scale).map(drop)
}

/// Creates a processor set, and returns its ID into `psid`.
pub unsafe fn pset_assign(
    psid: psetid_t,
    cpuid: cpuid_t,
    old_psid: Option<&mut psetid_t>,
) -> Result<(), Errno> {
    let psid = psid as usize;
    let cpuid = cpuid as usize;
    let old_psid_ptr = old_psid.map_or(0, |old_psid| old_psid as *mut psetid_t as usize);
    syscall3(SYS_PSET_ASSIGN, psid, cpuid, old_psid_ptr).map(drop)
}

/// Creates a processor set, and returns its ID into `psid`.
pub unsafe fn pset_create(psid: &mut psetid_t) -> Result<(), Errno> {
    let psid_ptr = psid as *mut psetid_t as usize;
    syscall1(SYS_PSET_CREATE, psid_ptr).map(drop)
}

/// Destroys the processor set specified by `psid`.
pub unsafe fn pset_destroy(psid: psetid_t) -> Result<(), Errno> {
    let psid = psid as usize;
    syscall1(SYS_PSET_DESTROY, psid).map(drop)
}

/// Process trace.
pub unsafe fn ptrace(request: i32, pid: pid_t, addr: usize, data: usize) -> Result<isize, Errno> {
    let request = request as usize;
    let pid = pid as usize;
    syscall4(SYS_PTRACE, request, pid, addr, data).map(|ret| ret as isize)
}

/// Write to a file descriptor without changing file offset.
///
/// # Example
///
/// ```
/// let path = "/tmp/nc-pwrite64";
/// let ret = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_WRONLY | nc::O_CREAT, 0o644) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
/// let buf = "Hello, Rust";
/// let ret = unsafe { nc::pwrite64(fd, buf.as_ptr() as usize, buf.len(), 0) };
/// assert!(ret.is_ok());
/// assert_eq!(ret, Ok(buf.len() as nc::ssize_t));
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, path, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn pwrite(fd: i32, buf: usize, count: size_t, offset: off_t) -> Result<ssize_t, Errno> {
    let fd = fd as usize;
    let offset = offset as usize;
    syscall4(SYS_PWRITE, fd, buf, count, offset).map(|ret| ret as ssize_t)
}

/// Write to a file descriptor without changing file offset.
///
/// # Example
///
/// ```
/// use core::ffi::c_void;
///
/// let path = "/etc/passwd";
/// let ret = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_RDONLY, 0) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
/// let mut buf = [[0_u8; 64]; 4];
/// let capacity = 4 * 64;
/// let mut iov = Vec::with_capacity(buf.len());
/// for ref mut item in (&mut buf).iter() {
///     iov.push(nc::iovec_t {
///         iov_len: item.len(),
///         iov_base: item.as_ptr() as *const c_void,
///     });
/// }
/// let ret = unsafe { nc::readv(fd, &mut iov) };
/// assert!(ret.is_ok());
/// assert_eq!(ret, Ok(capacity as nc::ssize_t));
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
///
/// let path_out = "/tmp/nc-pwritev";
/// let ret = unsafe { nc::openat(nc::AT_FDCWD, path_out, nc::O_WRONLY | nc::O_CREAT, 0o644) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
/// let ret = unsafe { nc::pwritev(fd, &iov, 0, iov.len() - 1) };
/// assert!(ret.is_ok());
/// assert_eq!(ret, Ok(capacity as nc::ssize_t));
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, path_out, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn pwritev(
    fd: i32,
    vec: &[iovec_t],
    pos_l: usize,
    pos_h: usize,
) -> Result<ssize_t, Errno> {
    let fd = fd as usize;
    let vec_ptr = vec.as_ptr() as usize;
    let vec_len = vec.len();
    syscall5(SYS_PWRITEV, fd, vec_ptr, vec_len, pos_l, pos_h).map(|ret| ret as ssize_t)
}

/// Restartable atomic sequences
pub unsafe fn rasctl(addr: uintptr_t, len: size_t, op: i32) -> Result<(), Errno> {
    let op = op as usize;
    syscall3(SYS_RASCTL, addr, len, op).map(drop)
}

/// Read from a file descriptor.
///
/// # Example
///
/// ```
/// let path = "/etc/passwd";
/// let ret = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_RDONLY, 0) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
/// let mut buf = [0_u8; 4 * 1024];
/// let ret = unsafe { nc::read(fd, buf.as_mut_ptr() as usize, buf.len()) };
/// assert!(ret.is_ok());
/// let n_read = ret.unwrap();
/// assert!(n_read <= buf.len() as nc::ssize_t);
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn read(fd: i32, buf_ptr: usize, count: size_t) -> Result<ssize_t, Errno> {
    let fd = fd as usize;
    syscall3(SYS_READ, fd, buf_ptr, count).map(|ret| ret as ssize_t)
}

/// Read value of a symbolic link.
///
/// # Example
///
/// ```
/// let oldname = "/etc/passwd";
/// let newname = "/tmp/nc-readlink";
/// let ret = unsafe { nc::symlinkat(oldname, nc::AT_FDCWD, newname) };
/// assert!(ret.is_ok());
/// let mut buf = [0_u8; nc::PATH_MAX as usize];
/// let buf_len = buf.len();
/// let ret = unsafe { nc::readlink(newname, &mut buf, buf_len) };
/// assert!(ret.is_ok());
/// let n_read = ret.unwrap() as usize;
/// assert_eq!(n_read, oldname.len());
/// assert_eq!(oldname.as_bytes(), &buf[0..n_read]);
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, newname, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn readlink<P: AsRef<Path>>(
    filename: P,
    buf: &mut [u8],
    buf_len: size_t,
) -> Result<ssize_t, Errno> {
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    let buf_ptr = buf.as_mut_ptr() as usize;
    syscall3(SYS_READLINK, filename_ptr, buf_ptr, buf_len).map(|ret| ret as ssize_t)
}

/// Read value of a symbolic link.
///
/// # Example
///
/// ```
/// let oldname = "/etc/passwd";
/// let newname = "/tmp/nc-readlinkat";
/// let ret = unsafe { nc::symlinkat(oldname, nc::AT_FDCWD, newname) };
/// assert!(ret.is_ok());
/// let mut buf = [0_u8; nc::PATH_MAX as usize];
/// let buf_len = buf.len();
/// let ret = unsafe { nc::readlinkat(nc::AT_FDCWD, newname, &mut buf, buf_len) };
/// assert!(ret.is_ok());
/// let n_read = ret.unwrap() as usize;
/// assert_eq!(n_read, oldname.len());
/// assert_eq!(oldname.as_bytes(), &buf[0..n_read]);
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, newname, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn readlinkat<P: AsRef<Path>>(
    dirfd: i32,
    filename: P,
    buf: &mut [u8],
    buf_len: size_t,
) -> Result<ssize_t, Errno> {
    let dirfd = dirfd as usize;
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    let buf_ptr = buf.as_mut_ptr() as usize;
    syscall4(SYS_READLINKAT, dirfd, filename_ptr, buf_ptr, buf_len).map(|ret| ret as ssize_t)
}

/// Read from a file descriptor into multiple buffers.
///
/// # Example
///
/// ```
/// use core::ffi::c_void;
///
/// let path = "/etc/passwd";
/// let ret = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_RDONLY, 0) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
/// let mut buf = [[0_u8; 64]; 4];
/// let capacity = 4 * 64;
/// let mut iov = Vec::with_capacity(buf.len());
/// for ref mut item in (&mut buf).iter() {
/// // TODO(Shaohua): Replace with as_mut_ptr()
///     iov.push(nc::iovec_t {
///         iov_len: item.len(),
///         iov_base: item.as_ptr() as *const c_void,
///     });
/// }
/// let ret = unsafe { nc::readv(fd, &mut iov) };
/// assert!(ret.is_ok());
/// assert_eq!(ret, Ok(capacity as nc::ssize_t));
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn readv(fd: i32, iov: &mut [iovec_t]) -> Result<ssize_t, Errno> {
    let fd = fd as usize;
    let iov_ptr = iov.as_mut_ptr() as usize;
    let len = iov.len();
    syscall3(SYS_READV, fd, iov_ptr, len).map(|ret| ret as ssize_t)
}

/// Reboot or enable/disable Ctrl-Alt-Del.
///
/// # Example
///
/// ```
/// let ret = unsafe {
///     nc::reboot(
///         nc::LINUX_REBOOT_MAGIC1,
///         nc::LINUX_REBOOT_MAGIC2,
///         nc::LINUX_REBOOT_CMD_RESTART,
///         0
///     )
/// };
/// assert!(ret.is_err());
/// assert_eq!(ret, Err(nc::EPERM));
/// ```
pub unsafe fn reboot(magic: i32, magci2: i32, cmd: u32, arg: usize) -> Result<(), Errno> {
    let magic = magic as usize;
    let magic2 = magci2 as usize;
    let cmd = cmd as usize;
    syscall4(SYS_REBOOT, magic, magic2, cmd, arg).map(drop)
}

/// Receive a message from a socket.
pub unsafe fn recvfrom(
    sockfd: i32,
    buf: &mut [u8],
    flags: i32,
    src_addr: &mut sockaddr_in_t,
    addrlen: &mut socklen_t,
) -> Result<ssize_t, Errno> {
    let sockfd = sockfd as usize;
    let buf_ptr = buf.as_mut_ptr() as usize;
    let buflen = buf.len();
    let flags = flags as usize;
    let src_addr_ptr = src_addr as *mut sockaddr_in_t as usize;
    let addrlen_ptr = addrlen as *mut socklen_t as usize;
    syscall6(
        SYS_RECVFROM,
        sockfd,
        buf_ptr,
        buflen,
        flags,
        src_addr_ptr,
        addrlen_ptr,
    )
    .map(|ret| ret as ssize_t)
}

/// Receives multile messages on a socket
pub unsafe fn recvmmsg(
    sockfd: i32,
    msgvec: &mut [mmsghdr_t],
    flags: i32,
    timeout: &mut timespec_t,
) -> Result<i32, Errno> {
    let sockfd = sockfd as usize;
    let msgvec_ptr = (msgvec as *mut [mmsghdr_t]).cast::<*mut mmsghdr_t>() as usize;
    let vlen = msgvec.len();
    let flags = flags as usize;
    let timeout_ptr = timeout as *mut timespec_t as usize;
    syscall5(SYS_RECVMMSG, sockfd, msgvec_ptr, vlen, flags, timeout_ptr).map(|ret| ret as i32)
}

/// Receive a msg from a socket.
pub unsafe fn recvmsg(sockfd: i32, msg: &mut msghdr_t, flags: i32) -> Result<ssize_t, Errno> {
    let sockfd = sockfd as usize;
    let msg_ptr = msg as *mut msghdr_t as usize;
    let flags = flags as usize;
    syscall3(SYS_RECVMSG, sockfd, msg_ptr, flags).map(|ret| ret as ssize_t)
}

/// Remove an extended attribute.
///
/// # Example
///
/// ```
/// let path = "/tmp/nc-removexattr";
/// let ret = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_WRONLY | nc::O_CREAT, 0o644) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// let attr_name = "user.creator";
/// let attr_value = "nc-0.0.1";
/// let flags = nc::XATTR_CREATE;
/// let ret = unsafe {
///     nc::setxattr(
///         path,
///         &attr_name,
///         attr_value.as_ptr() as usize,
///         attr_value.len(),
///         flags,
///     )
/// };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::removexattr(path, attr_name) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, path, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn removexattr<P: AsRef<Path>>(filename: P, name: P) -> Result<(), Errno> {
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    let name = CString::new(name.as_ref());
    let name_ptr = name.as_ptr() as usize;
    syscall2(SYS_REMOVEXATTR, filename_ptr, name_ptr).map(drop)
}

/// Change name or location of a file.
///
/// # Example
///
/// ```
/// let path = "/tmp/nc-rename";
/// let ret = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_WRONLY | nc::O_CREAT, 0o644) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// let new_path = "/tmp/nc-rename-new";
/// let ret = unsafe { nc::rename(path, new_path) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, new_path, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn rename<P: AsRef<Path>>(oldfilename: P, newfilename: P) -> Result<(), Errno> {
    let oldfilename = CString::new(oldfilename.as_ref());
    let oldfilename_ptr = oldfilename.as_ptr() as usize;
    let newfilename = CString::new(newfilename.as_ref());
    let newfilename_ptr = newfilename.as_ptr() as usize;
    syscall2(SYS_RENAME, oldfilename_ptr, newfilename_ptr).map(drop)
}

/// Change name or location of a file.
///
/// # Example
///
/// ```
/// let path = "/tmp/nc-renameat";
/// let ret = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_WRONLY | nc::O_CREAT, 0o644) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// let new_path = "/tmp/nc-renameat-new";
/// let ret = unsafe { nc::renameat(nc::AT_FDCWD, path, nc::AT_FDCWD, new_path) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, new_path, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn renameat<P: AsRef<Path>>(
    olddfd: i32,
    oldfilename: P,
    newdfd: i32,
    newfilename: P,
) -> Result<(), Errno> {
    let olddfd = olddfd as usize;
    let oldfilename = CString::new(oldfilename.as_ref());
    let oldfilename_ptr = oldfilename.as_ptr() as usize;
    let newdfd = newdfd as usize;
    let newfilename = CString::new(newfilename.as_ref());
    let newfilename_ptr = newfilename.as_ptr() as usize;
    syscall4(
        SYS_RENAMEAT,
        olddfd,
        oldfilename_ptr,
        newdfd,
        newfilename_ptr,
    )
    .map(drop)
}

/// Revoke file access
pub unsafe fn revoke<P: AsRef<Path>>(path: P) -> Result<(), Errno> {
    let path = CString::new(path.as_ref());
    let path_ptr = path.as_ptr() as usize;
    syscall1(SYS_REVOKE, path_ptr).map(drop)
}

/// Delete a directory.
///
/// # Example
///
/// ```
/// let path = "/tmp/nc-rmdir";
/// let ret = unsafe { nc::mkdirat(nc::AT_FDCWD, path, 0o755) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::rmdir(path) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn rmdir<P: AsRef<Path>>(filename: P) -> Result<(), Errno> {
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    syscall1(SYS_RMDIR, filename_ptr).map(drop)
}

/// Yield the processor.
///
/// # Example
///
/// ```
/// let ret = unsafe { nc::sched_yield() };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn sched_yield() -> Result<(), Errno> {
    syscall0(SYS_SCHED_YIELD).map(drop)
}

pub unsafe fn semconfig(flag: i32) -> Result<i32, Errno> {
    let flag = flag as usize;
    syscall1(SYS_SEMCONFIG, flag).map(|val| val as i32)
}

/// Get a System V semphore set identifier.
pub unsafe fn semget(key: key_t, nsems: i32, semflg: i32) -> Result<i32, Errno> {
    let key = key as usize;
    let nsems = nsems as usize;
    let semflg = semflg as usize;
    syscall3(SYS_SEMGET, key, nsems, semflg).map(|ret| ret as i32)
}

/// System V semphore operations.
pub unsafe fn semop(semid: i32, sops: &mut [sembuf_t]) -> Result<(), Errno> {
    let semid = semid as usize;
    let sops_ptr = sops.as_ptr() as usize;
    let nops = sops.len();
    syscall3(SYS_SEMOP, semid, sops_ptr, nops).map(drop)
}

/// Send multiple messages on a socket
pub unsafe fn sendmmsg(sockfd: i32, msgvec: &mut [mmsghdr_t], flags: i32) -> Result<i32, Errno> {
    let sockfd = sockfd as usize;
    let msgvec_ptr = (msgvec as *mut [mmsghdr_t]).cast::<*mut mmsghdr_t>() as usize;
    let vlen = msgvec.len();
    let flags = flags as usize;
    syscall4(SYS_SENDMMSG, sockfd, msgvec_ptr, vlen, flags).map(|ret| ret as i32)
}

/// Send a message on a socket. Allow sending ancillary data.
pub unsafe fn sendmsg(sockfd: i32, msg: &msghdr_t, flags: i32) -> Result<ssize_t, Errno> {
    let sockfd = sockfd as usize;
    let msg_ptr = msg as *const msghdr_t as usize;
    let flags = flags as usize;
    syscall3(SYS_SENDMSG, sockfd, msg_ptr, flags).map(|ret| ret as ssize_t)
}

/// Send a message on a socket.
pub unsafe fn sendto(
    sockfd: i32,
    buf: &[u8],
    len: size_t,
    flags: i32,
    dest_addr: &sockaddr_in_t,
    addrlen: socklen_t,
) -> Result<ssize_t, Errno> {
    let sockfd = sockfd as usize;
    let buf_ptr = buf.as_ptr() as usize;
    let flags = flags as usize;
    let dest_addr_ptr = dest_addr as *const sockaddr_in_t as usize;
    let addrlen = addrlen as usize;
    syscall6(
        SYS_SENDTO,
        sockfd,
        buf_ptr,
        len,
        flags,
        dest_addr_ptr,
        addrlen,
    )
    .map(|ret| ret as ssize_t)
}

/// Set user thread context.
pub unsafe fn setcontext(ctx: &ucontext_t) -> Result<(), Errno> {
    let ctx_ptr = ctx as *const ucontext_t as usize;
    syscall1(SYS_SETCONTEXT, ctx_ptr).map(drop)
}

/// Set the effective group ID of the calling process to `gid`.
///
/// # Example
///
/// ```
/// let ret = unsafe { nc::setegid(0) };
/// assert!(ret.is_err());
/// assert_eq!(ret, Err(nc::EPERM));
/// ```
pub unsafe fn setegid(gid: gid_t) -> Result<(), Errno> {
    let gid = gid as usize;
    syscall1(SYS_SETEGID, gid).map(drop)
}

/// Set the effective user ID of the calling process to `uid`.
///
/// # Example
///
/// ```
/// let ret = unsafe { nc::seteuid(0) };
/// assert_eq!(ret, Err(nc::EPERM));
/// ```
pub unsafe fn seteuid(uid: uid_t) -> Result<(), Errno> {
    let uid = uid as usize;
    syscall1(SYS_SETEUID, uid).map(drop)
}

/// Set the group ID of the calling process to `gid`.
///
/// # Example
///
/// ```
/// let ret = unsafe { nc::setgid(0) };
/// assert!(ret.is_err());
/// assert_eq!(ret, Err(nc::EPERM));
/// ```
pub unsafe fn setgid(gid: gid_t) -> Result<(), Errno> {
    let gid = gid as usize;
    syscall1(SYS_SETGID, gid).map(drop)
}

/// Set list of supplementary group Ids.
///
/// # Example
///
/// ```
/// let list = [0, 1, 2];
/// let ret = unsafe { nc::setgroups(&list) };
/// assert!(ret.is_err());
/// assert_eq!(ret, Err(nc::EPERM));
/// ```
pub unsafe fn setgroups(group_list: &[gid_t]) -> Result<(), Errno> {
    let group_len = group_list.len();
    let group_ptr = group_list.as_ptr() as usize;
    syscall2(SYS_SETGROUPS, group_len, group_ptr).map(drop)
}

/// Set the process group ID (PGID) of the process specified by `pid` to `pgid`.
///
/// # Example
///
/// ```
/// let ret = unsafe { nc::setpgid(nc::getpid(), 1) };
/// assert!(ret.is_err());
/// assert_eq!(ret, Err(nc::EPERM));
/// ```
pub unsafe fn setpgid(pid: pid_t, pgid: pid_t) -> Result<(), Errno> {
    let pid = pid as usize;
    let pgid = pgid as usize;
    syscall2(SYS_SETPGID, pid, pgid).map(drop)
}

/// Set program scheduling priority.
///
/// # Example
///
/// ```
/// let ret = unsafe { nc::setpriority(nc::PRIO_PROCESS, nc::getpid(), -19) };
/// assert!(ret.is_err());
/// assert_eq!(ret, Err(nc::EACCES))
/// ```
pub unsafe fn setpriority(which: i32, who: i32, prio: i32) -> Result<(), Errno> {
    let which = which as usize;
    let who = who as usize;
    let prio = prio as usize;
    syscall3(SYS_SETPRIORITY, which, who, prio).map(drop)
}

/// Set real and effective group IDs of the calling process.
///
/// # Example
///
/// ```
/// let ret = unsafe { nc::setregid(0, 0) };
/// assert_eq!(ret, Err(nc::EPERM));
/// ```
pub unsafe fn setregid(rgid: gid_t, egid: gid_t) -> Result<(), Errno> {
    let rgid = rgid as usize;
    let egid = egid as usize;
    syscall2(SYS_SETREGID, rgid, egid).map(drop)
}

/// Set real and effective user IDs of the calling process.
///
/// # Example
///
/// ```
/// let ret = unsafe { nc::setreuid(0, 0) };
/// assert_eq!(ret, Err(nc::EPERM));
/// ```
pub unsafe fn setreuid(ruid: uid_t, euid: uid_t) -> Result<(), Errno> {
    let ruid = ruid as usize;
    let euid = euid as usize;
    syscall2(SYS_SETREUID, ruid, euid).map(drop)
}

/// Set resource limit.
///
/// # Example
///
/// ```
/// let rlimit = nc::rlimit_t {
///     rlim_cur: 128,
///     rlim_max: 128,
/// };
/// let ret = unsafe { nc::setrlimit(nc::RLIMIT_NOFILE, &rlimit) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn setrlimit(resource: i32, rlimit: &rlimit_t) -> Result<(), Errno> {
    let resource = resource as usize;
    let rlimit_ptr = rlimit as *const rlimit_t as usize;
    syscall2(SYS_SETRLIMIT, resource, rlimit_ptr).map(drop)
}

/// Create a new session if the calling process is not a process group leader.
///
/// # Example
///
/// ```
/// let ret = unsafe { nc::setsid() };
/// assert!(ret.is_ok());
/// let pid = unsafe { nc::getpid() };
/// assert_eq!(ret, Ok(pid));
/// ```
pub unsafe fn setsid() -> Result<pid_t, Errno> {
    syscall0(SYS_SETSID).map(|ret| ret as pid_t)
}

/// Set options on sockets.
///
/// # Example
///
/// ```
/// let socket_fd = unsafe { nc::socket(nc::AF_INET, nc::SOCK_STREAM, 0) };
/// assert!(socket_fd.is_ok());
/// let socket_fd = socket_fd.unwrap();
///
/// // Enable tcp fast open.
/// let queue_len: i32 = 5;
/// let queue_len_ptr = &queue_len as *const i32 as usize;
/// let ret = unsafe {
///     nc::setsockopt(
///         socket_fd,
///         nc::IPPROTO_TCP,
///         nc::TCP_FASTOPEN,
///         queue_len_ptr,
///         std::mem::size_of_val(&queue_len) as u32,
///     )
/// };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::close(socket_fd) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn setsockopt(
    sockfd: i32,
    level: i32,
    optname: i32,
    optval: usize,
    optlen: socklen_t,
) -> Result<(), Errno> {
    let sockfd = sockfd as usize;
    let level = level as usize;
    let optname = optname as usize;
    let optlen = optlen as usize;
    syscall5(SYS_SETSOCKOPT, sockfd, level, optname, optval, optlen).map(drop)
}

/// Set user ID of the calling process to `uid`.
///
/// # Example
///
/// ```
/// let ret = unsafe { nc::setuid(0) };
/// assert_eq!(ret, Err(nc::EPERM));
/// ```
pub unsafe fn setuid(uid: uid_t) -> Result<(), Errno> {
    let uid = uid as usize;
    syscall1(SYS_SETUID, uid).map(drop)
}

/// Set extended attribute value.
///
/// # Example
///
/// ```
/// let path = "/tmp/nc-setxattr";
/// let ret = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_WRONLY | nc::O_CREAT, 0o644) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// let attr_name = "user.creator";
/// let attr_value = "nc-0.0.1";
/// //let flags = 0;
/// let flags = nc::XATTR_CREATE;
/// let ret = unsafe {
///     nc::setxattr(
///         path,
///         &attr_name,
///         attr_value.as_ptr() as usize,
///         attr_value.len(),
///         flags,
///     )
/// };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, path, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn setxattr<P: AsRef<Path>>(
    filename: P,
    name: P,
    value: usize,
    size: size_t,
    flags: i32,
) -> Result<(), Errno> {
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    let name = CString::new(name.as_ref());
    let name_ptr = name.as_ptr() as usize;
    let flags = flags as usize;
    syscall5(SYS_SETXATTR, filename_ptr, name_ptr, value, size, flags).map(drop)
}

/// Attach the System V shared memory segment.
///
/// # Example
///
/// ```
/// let size = 4 * nc::PAGE_SIZE;
/// let flags = nc::IPC_CREAT | nc::IPC_EXCL | 0o600;
/// let ret = unsafe { nc::shmget(nc::IPC_PRIVATE, size, flags) };
/// assert!(ret.is_ok());
/// let shmid = ret.unwrap();
///
/// let addr: usize = 0;
/// let ret = unsafe { nc::shmat(shmid, addr, 0) };
/// assert!(ret.is_ok());
/// let addr = ret.unwrap();
///
/// let mut buf = nc::shmid_ds_t::default();
/// let ret = unsafe { nc::shmctl(shmid, nc::IPC_STAT, &mut buf) };
/// assert!(ret.is_ok());
///
/// let ret = unsafe { nc::shmdt(addr) };
/// assert!(ret.is_ok());
///
/// let ret = unsafe { nc::shmctl(shmid, nc::IPC_RMID, &mut buf) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn shmat(shmid: i32, shmaddr: usize, shmflg: i32) -> Result<usize, Errno> {
    let shmid = shmid as usize;
    let shmflg = shmflg as usize;
    syscall3(SYS_SHMAT, shmid, shmaddr, shmflg)
}

/// Detach the System V shared memory segment.
///
/// # Example
///
/// ```
/// let size = 4 * nc::PAGE_SIZE;
/// let flags = nc::IPC_CREAT | nc::IPC_EXCL | 0o600;
/// let ret = unsafe { nc::shmget(nc::IPC_PRIVATE, size, flags) };
/// assert!(ret.is_ok());
/// let shmid = ret.unwrap();
///
/// let addr: usize = 0;
/// let ret = unsafe { nc::shmat(shmid, addr, 0) };
/// assert!(ret.is_ok());
/// let addr = ret.unwrap();
///
/// let mut buf = nc::shmid_ds_t::default();
/// let ret = unsafe { nc::shmctl(shmid, nc::IPC_STAT, &mut buf) };
/// assert!(ret.is_ok());
///
/// let ret = unsafe { nc::shmdt(addr) };
/// assert!(ret.is_ok());
///
/// let ret = unsafe { nc::shmctl(shmid, nc::IPC_RMID, &mut buf) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn shmdt(shmaddr: usize) -> Result<(), Errno> {
    syscall1(SYS_SHMDT, shmaddr).map(drop)
}

/// Allocates a System V shared memory segment.
///
/// # Example
///
/// ```
/// let size = 4 * nc::PAGE_SIZE;
/// let flags = nc::IPC_CREAT | nc::IPC_EXCL | 0o600;
/// let ret = unsafe { nc::shmget(nc::IPC_PRIVATE, size, flags) };
/// assert!(ret.is_ok());
/// let _shmid = ret.unwrap();
/// ```
pub unsafe fn shmget(key: key_t, size: size_t, shmflg: i32) -> Result<i32, Errno> {
    let key = key as usize;
    let shmflg = shmflg as usize;
    syscall3(SYS_SHMGET, key, size, shmflg).map(|ret| ret as i32)
}

/// Shutdown part of a full-duplex connection.
pub unsafe fn shutdown(sockfd: i32, how: i32) -> Result<(), Errno> {
    let sockfd = sockfd as usize;
    let how = how as usize;
    syscall2(SYS_SHUTDOWN, sockfd, how).map(drop)
}

/// Queue a signal to a process (REALTIME)
pub unsafe fn sigqueueinfo(pid: pid_t, info: &siginfo_t) -> Result<(), Errno> {
    let pid = pid as usize;
    let info_ptr = info as *const siginfo_t as usize;
    syscall2(SYS_SIGQUEUEINFO, pid, info_ptr).map(drop)
}

/// Create a pair of connected socket.
pub unsafe fn socketpair(
    domain: i32,
    type_: i32,
    protocol: i32,
    sv: [i32; 2],
) -> Result<(), Errno> {
    let domain = domain as usize;
    let type_ = type_ as usize;
    let protocol = protocol as usize;
    let sv_ptr = sv.as_ptr() as usize;
    syscall4(SYS_SOCKETPAIR, domain, type_, protocol, sv_ptr).map(drop)
}

/// Get file system statistics
pub unsafe fn statvfs1<P: AsRef<Path>>(
    path: P,
    buf: &mut statvfs_t,
    flags: i32,
) -> Result<(), Errno> {
    let path = CString::new(path.as_ref());
    let path_ptr = path.as_ptr() as usize;
    let buf_ptr = buf as *mut statvfs_t as usize;
    let flags = flags as usize;
    syscall3(SYS_STATVFS1, path_ptr, buf_ptr, flags).map(drop)
}

/// Modify swap configuration
pub unsafe fn swapctl(cmd: i32, arg: usize, misc: i32) -> Result<i32, Errno> {
    let cmd = cmd as usize;
    let misc = misc as usize;
    syscall3(SYS_SWAPCTL, cmd, arg, misc).map(|val| val as i32)
}

/// Make a new name for a file.
///
/// # Example
///
/// ```
/// let oldname = "/etc/passwd";
/// let newname = "/tmp/nc-symlink";
/// let ret = unsafe { nc::symlink(oldname, newname) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, newname,0 ) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn symlink<P: AsRef<Path>>(oldname: P, newname: P) -> Result<(), Errno> {
    let oldname = CString::new(oldname.as_ref());
    let oldname_ptr = oldname.as_ptr() as usize;
    let newname = CString::new(newname.as_ref());
    let newname_ptr = newname.as_ptr() as usize;
    syscall2(SYS_SYMLINK, oldname_ptr, newname_ptr).map(drop)
}

/// Make a new name for a file.
///
/// # Example
///
/// ```
/// let oldname = "/etc/passwd";
/// let newname = "/tmp/nc-symlinkat";
/// let ret = unsafe { nc::symlinkat(oldname, nc::AT_FDCWD, newname) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, newname, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn symlinkat<P: AsRef<Path>>(
    oldname: P,
    newdirfd: i32,
    newname: P,
) -> Result<(), Errno> {
    let oldname = CString::new(oldname.as_ref());
    let oldname_ptr = oldname.as_ptr() as usize;
    let newname = CString::new(newname.as_ref());
    let newname_ptr = newname.as_ptr() as usize;
    let newdirfd = newdirfd as usize;
    syscall3(SYS_SYMLINKAT, oldname_ptr, newdirfd, newname_ptr).map(drop)
}

/// Commit filesystem caches to disk.
///
/// # Example
///
/// ```
/// let ret = unsafe { nc::sync() };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn sync() -> Result<(), Errno> {
    syscall0(SYS_SYNC).map(drop)
}

/// Architecture dependent system call.
pub unsafe fn sysarch(number: i32, args: usize) -> Result<usize, Errno> {
    let number = number as usize;
    syscall2(SYS_SYSARCH, number, args)
}

/// Create a per-process timer
///
/// # Example
///
/// ```
/// let mut timerid = nc::timer_t::default();
/// let ret = unsafe { nc::timer_create(nc::CLOCK_MONOTONIC, None, &mut timerid) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn timer_create(
    clock: clockid_t,
    event: Option<&mut sigevent_t>,
    timer_id: &mut timer_t,
) -> Result<(), Errno> {
    let clock = clock as usize;
    let event_ptr = event.map_or(0, |event| event as *mut sigevent_t as usize);
    let timer_id_ptr = timer_id as *mut timer_t as usize;
    syscall3(SYS_TIMER_CREATE, clock, event_ptr, timer_id_ptr).map(drop)
}

/// Delete a per-process timer
///
/// # Example
///
/// ```
/// let mut timer_id = nc::timer_t::default();
/// let ret = unsafe { nc::timer_create(nc::CLOCK_MONOTONIC, None, &mut timer_id) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::timer_delete(timer_id) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn timer_delete(timer_id: timer_t) -> Result<(), Errno> {
    let timer_id = timer_id as usize;
    syscall1(SYS_TIMER_DELETE, timer_id).map(drop)
}

/// Get overrun count for a per-process timer.
///
/// # Example
///
/// ```
/// use core::mem::size_of;
///
/// fn handle_alarm(signum: i32) {
///     assert_eq!(signum, nc::SIGALRM);
/// }
///
/// fn main() {
///     const TIMER_SIG: i32 = nc::SIGRTMAX;
///
///     let sa = nc::sigaction_t {
///         sa_flags: nc::SA_SIGINFO,
///         sa_handler: handle_alarm as nc::sighandler_t,
///         ..nc::sigaction_t::default()
///     };
///     let mut old_sa = nc::sigaction_t::default();
///     let ret = unsafe { nc::rt_sigaction(TIMER_SIG, &sa, &mut old_sa, size_of::<nc::sigset_t>()) };
///     assert!(ret.is_ok());
///
///     let tid = nc::itimerspec_t {
///         it_interval: nc::timespec_t::default(),
///         it_value: nc::timespec_t {
///             tv_sec: 1,
///             tv_nsec: 0,
///         },
///     };
///     let mut ev = nc::sigevent_t {
///         sigev_value: nc::sigval_t {
///             sival_ptr: &tid as *const nc::itimerspec_t as usize,
///         },
///         sigev_signo: TIMER_SIG,
///         sigev_notify: nc::SIGEV_SIGNAL,
///         sigev_un: nc::sigev_un_t::default(),
///     };
///     let mut timer_id = nc::timer_t::default();
///     let ret = unsafe { nc::timer_create(nc::CLOCK_MONOTONIC, Some(&mut ev), &mut timer_id) };
///     assert!(ret.is_ok());
///     println!("timer id: {:?}", timer_id);
///
///     let flags = 0;
///     let time = nc::itimerspec_t {
///         it_interval: nc::timespec_t::default(),
///         it_value: nc::timespec_t {
///             tv_sec: 1,
///             tv_nsec: 0,
///         },
///     };
///     let ret = unsafe { nc::timer_settime(timer_id, flags, &time, None) };
///     assert!(ret.is_ok());
///
///     let mut cur_time = nc::itimerspec_t::default();
///     let ret = unsafe { nc::timer_gettime(timer_id, &mut cur_time) };
///     assert!(ret.is_ok());
///     println!("cur time: {:?}", cur_time);
///
///     let mask = nc::sigset_t::default();
///     let _ret = unsafe { nc::rt_sigsuspend(&mask, size_of::<nc::sigset_t>()) };
///
///     let ret = unsafe { nc::timer_getoverrun(timer_id) };
///     assert!(ret.is_ok());
///     assert_eq!(ret, Ok(0));
///
///     let ret = unsafe { nc::timer_delete(timer_id) };
///     assert!(ret.is_ok());
/// }
/// ```
pub unsafe fn timer_getoverrun(timer_id: timer_t) -> Result<i32, Errno> {
    let timer_id = timer_id as usize;
    syscall1(SYS_TIMER_GETOVERRUN, timer_id).map(|ret| ret as i32)
}

/// Truncate a file to a specified length.
///
/// # Example
///
/// ```
/// let path = "/tmp/nc-truncate";
/// let ret = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_WRONLY | nc::O_CREAT, 0o644) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::truncate(path, 64 * 1024) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, path, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn truncate<P: AsRef<Path>>(filename: P, length: off_t) -> Result<(), Errno> {
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    let length = length as usize;
    syscall2(SYS_TRUNCATE, filename_ptr, length).map(drop)
}

/// Set file mode creation mask.
///
/// # Example
///
/// ```
/// let new_mask = 0o077;
/// let ret = unsafe { nc::umask(new_mask) };
/// assert!(ret.is_ok());
/// let old_mask = ret.unwrap();
/// let ret = unsafe { nc::umask(old_mask) };
/// assert_eq!(ret, Ok(new_mask));
/// ```
pub unsafe fn umask(mode: mode_t) -> Result<mode_t, Errno> {
    let mode = mode as usize;
    syscall1(SYS_UMASK, mode).map(|ret| ret as mode_t)
}

/// Attempt to recover a deleted file
pub unsafe fn undelete<P: AsRef<Path>>(path: P) -> Result<(), Errno> {
    let path = CString::new(path.as_ref());
    let path_ptr = path.as_ptr() as usize;
    syscall1(SYS_UNDELETE, path_ptr).map(drop)
}

/// Delete a name and possibly the file it refers to.
///
/// # Example
///
/// ```
/// let path = "/tmp/nc-unlink";
/// let ret = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_WRONLY | nc::O_CREAT, 0o644) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, path, 0) };;
/// assert!(ret.is_ok());
/// ```
pub unsafe fn unlink<P: AsRef<Path>>(filename: P) -> Result<(), Errno> {
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    syscall1(SYS_UNLINK, filename_ptr).map(drop)
}

/// Delete a name and possibly the file it refers to.
///
/// # Example
///
/// ```
/// let path = "/tmp/nc-unlinkat";
/// let ret = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_WRONLY | nc::O_CREAT, 0o644) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// // /tmp folder is not empty, so this call always returns error.
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, path, nc::AT_REMOVEDIR) };
/// assert!(ret.is_err());
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, path, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn unlinkat<P: AsRef<Path>>(dfd: i32, filename: P, flag: i32) -> Result<(), Errno> {
    let dfd = dfd as usize;
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    let flag = flag as usize;
    syscall3(SYS_UNLINKAT, dfd, filename_ptr, flag).map(drop)
}

/// Unmount filesystem.
pub unsafe fn unmount<P: AsRef<Path>>(path: P, flags: i32) -> Result<(), Errno> {
    let path = CString::new(path.as_ref());
    let path_ptr = path.as_ptr() as usize;
    let flags = flags as usize;
    syscall2(SYS_UNMOUNT, path_ptr, flags).map(drop)
}

/// Change time timestamps with nanosecond precision.
///
/// # Example
///
/// ```
/// let path = "/tmp/nc-utimesat";
/// let ret = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_WRONLY | nc::O_CREAT, 0o644) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// let times = [
///     nc::timespec_t {
///         tv_sec: 100,
///         tv_nsec: 0,
///     },
///     nc::timespec_t {
///         tv_sec: 10,
///         tv_nsec: 0,
///     },
/// ];
/// let flags = nc::AT_SYMLINK_NOFOLLOW;
/// let ret = unsafe { nc::utimensat(nc::AT_FDCWD, path, &times, flags) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, path, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn utimensat<P: AsRef<Path>>(
    dirfd: i32,
    filename: P,
    times: &[timespec_t; 2],
    flags: i32,
) -> Result<(), Errno> {
    let dirfd = dirfd as usize;
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    let times_ptr = times.as_ptr() as usize;
    let flags = flags as usize;
    syscall4(SYS_UTIMENSAT, dirfd, filename_ptr, times_ptr, flags).map(drop)
}

/// Insert user record in ktrace log
pub unsafe fn utrace(addr: usize, len: size_t) -> Result<(), Errno> {
    syscall2(SYS_UTRACE, addr, len).map(drop)
}

/// Generate universally unique identifiers
pub unsafe fn uuidgen(store: &mut [uuid_t]) -> Result<(), Errno> {
    let store_ptr = store.as_mut_ptr() as usize;
    let count = store.len();
    syscall2(SYS_UUIDGEN, store_ptr, count).map(drop)
}

pub unsafe fn vadvise(anom: i32) -> Result<i32, Errno> {
    let anom = anom as usize;
    syscall1(SYS_VADVISE, anom).map(|val| val as i32)
}

/// Create a child process and wait until it is terminated.
pub unsafe fn vfork() -> Result<pid_t, Errno> {
    syscall0(SYS_VFORK).map(|ret| ret as pid_t)
}

/// Wait for process to change state.
pub unsafe fn wait6(
    idtype: idtype_t,
    id: id_t,
    status: &mut i32,
    options: i32,
    wrusage: &mut wrusage_t,
    info: &mut siginfo_t,
) -> Result<pid_t, Errno> {
    let idtype = idtype as usize;
    let id = id as usize;
    let status_ptr = status as *mut i32 as usize;
    let options = options as usize;
    let wrusage_ptr = wrusage as *mut wrusage_t as usize;
    let info_ptr = info as *mut siginfo_t as usize;
    syscall6(
        SYS_WAIT6,
        idtype,
        id,
        status_ptr,
        options,
        wrusage_ptr,
        info_ptr,
    )
    .map(|ret| ret as pid_t)
}

/// Write to a file descriptor.
///
/// # Example
///
/// ```
/// let path = "/tmp/nc-write";
/// let ret = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_CREAT | nc::O_WRONLY, 0o644) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
/// let msg = "Hello, Rust!";
/// let ret = unsafe { nc::write(fd, msg.as_ptr() as usize, msg.len()) };
/// assert!(ret.is_ok());
/// assert_eq!(ret, Ok(msg.len() as nc::ssize_t));
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, path, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn write(fd: i32, buf_ptr: usize, count: size_t) -> Result<ssize_t, Errno> {
    let fd = fd as usize;
    syscall3(SYS_WRITE, fd, buf_ptr, count).map(|ret| ret as ssize_t)
}

/// Write to a file descriptor from multiple buffers.
///
/// # Example
///
/// ```
/// use core::ffi::c_void;
///
/// let path = "/etc/passwd";
/// let ret = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_RDONLY, 0) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
/// let mut buf = [[0_u8; 64]; 4];
/// let capacity = 4 * 64;
/// let mut iov = Vec::with_capacity(buf.len());
/// for ref mut item in (&mut buf).iter() {
///     iov.push(nc::iovec_t {
///         iov_len: item.len(),
///         iov_base: item.as_ptr() as *const c_void,
///     });
/// }
/// let ret = unsafe { nc::readv(fd, &mut iov) };
/// assert!(ret.is_ok());
/// assert_eq!(ret, Ok(capacity as nc::ssize_t));
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
///
/// let path_out = "/tmp/nc-writev";
/// let ret = unsafe { nc::openat(nc::AT_FDCWD, path_out, nc::O_WRONLY | nc::O_CREAT, 0o644) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
/// let ret = unsafe { nc::writev(fd, &iov) };
/// assert!(ret.is_ok());
/// assert_eq!(ret, Ok(capacity as nc::ssize_t));
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, path_out, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn writev(fd: i32, iov: &[iovec_t]) -> Result<ssize_t, Errno> {
    let fd = fd as usize;
    let iov_ptr = iov.as_ptr() as usize;
    let len = iov.len();
    syscall3(SYS_WRITEV, fd, iov_ptr, len).map(|ret| ret as ssize_t)
}

/// Close a semaphore.
pub unsafe fn _ksem_close(id: intptr_t) -> Result<(), Errno> {
    let id = id as usize;
    syscall1(SYS__KSEM_CLOSE, id).map(drop)
}

/// Destroy an unnamed semaphore
pub unsafe fn _ksem_destroy(id: intptr_t) -> Result<(), Errno> {
    let id = id as usize;
    syscall1(SYS__KSEM_DESTROY, id).map(drop)
}

/// Get the value of a semaphore
pub unsafe fn _ksem_getvalue(id: intptr_t, value: &mut u32) -> Result<(), Errno> {
    let id = id as usize;
    let value_ptr = value as *mut u32 as usize;
    syscall2(SYS__KSEM_GETVALUE, id, value_ptr).map(drop)
}

/// Initialize an unnamed semaphore.
pub unsafe fn _ksem_init(value: u32, id: &mut intptr_t) -> Result<(), Errno> {
    let value = value as usize;
    let id_ptr = id as *mut intptr_t as usize;
    syscall2(SYS__KSEM_INIT, value, id_ptr).map(drop)
}

/// Initialize and open a named semaphore.
pub unsafe fn _ksem_open<P: AsRef<Path>>(
    name: P,
    flag: i32,
    mode: mode_t,
    value: u32,
    id: &mut intptr_t,
) -> Result<(), Errno> {
    let name = CString::new(name.as_ref());
    let name_ptr = name.as_ptr() as usize;
    let flag = flag as usize;
    let mode = mode as usize;
    let value = value as usize;
    let id_ptr = id as *mut intptr_t as usize;
    syscall5(SYS__KSEM_OPEN, name_ptr, flag, mode, value, id_ptr).map(drop)
}

/// Unlock a semaphore.
pub unsafe fn _ksem_post(id: intptr_t) -> Result<(), Errno> {
    let id = id as usize;
    syscall1(SYS__KSEM_POST, id).map(drop)
}

/// Lock a semaphore.
pub unsafe fn _ksem_timedwait(id: intptr_t, abstime: &timespec_t) -> Result<(), Errno> {
    let id = id as usize;
    let abstime_ptr = abstime as *const timespec_t as usize;
    syscall2(SYS__KSEM_TIMEDWAIT, id, abstime_ptr).map(drop)
}

/// Lock a semaphore.
pub unsafe fn _ksem_trywait(id: intptr_t) -> Result<(), Errno> {
    let id = id as usize;
    syscall1(SYS__KSEM_TRYWAIT, id).map(drop)
}

/// Remove a named semaphore.
pub unsafe fn _ksem_unlink<P: AsRef<Path>>(name: P) -> Result<(), Errno> {
    let name = CString::new(name.as_ref());
    let name_ptr = name.as_ptr() as usize;
    syscall1(SYS__KSEM_UNLINK, name_ptr).map(drop)
}

/// Lock a semaphore.
pub unsafe fn _ksem_wait(id: intptr_t) -> Result<(), Errno> {
    let id = id as usize;
    syscall1(SYS__KSEM_WAIT, id).map(drop)
}

pub unsafe fn _lwp_continue(target: lwpid_t) -> Result<(), Errno> {
    let target = target as usize;
    syscall1(SYS__LWP_CONTINUE, target).map(drop)
}

/// Crate a new thread.
pub unsafe fn _lwp_craete(
    ctx: &mut ucontext_t,
    flags: usize,
    id: &mut lwpid_t,
) -> Result<(), Errno> {
    let ctx_ptr = ctx as *mut ucontext_t as usize;
    let id_ptr = id as *mut lwpid_t as usize;
    syscall3(SYS__LWP_CREATE, ctx_ptr, flags, id_ptr).map(drop)
}

pub unsafe fn _lwp_detach(target: lwpid_t) -> Result<(), Errno> {
    let target = target as usize;
    syscall1(SYS__LWP_DETACH, target).map(drop)
}

/// Terminate thread.
pub unsafe fn _lwp_exit() -> Result<i32, Errno> {
    syscall0(SYS__LWP_EXIT).map(|val| val as i32)
}

pub unsafe fn _lwp_getname(target: lwpid_t, buf: &mut [u8]) -> Result<i32, Errno> {
    let target = target as usize;
    let buf_ptr = buf.as_mut_ptr() as usize;
    let buf_len = buf.len();
    syscall3(SYS__LWP_GETNAME, target, buf_ptr, buf_len).map(|val| val as i32)
}

pub unsafe fn _lwp_getprivate() -> Result<uintptr_t, Errno> {
    syscall0(SYS__LWP_GETPRIVATE)
}

pub unsafe fn _lwp_kill(target: lwpid_t, signo: i32) -> Result<(), Errno> {
    let target = target as usize;
    let signo = signo as usize;
    syscall2(SYS__LWP_KILL, target, signo).map(drop)
}

/// Get thread id of current thread.
pub unsafe fn _lwp_self() -> Result<lwpid_t, Errno> {
    syscall0(SYS__LWP_SELF).map(|val| val as lwpid_t)
}

pub unsafe fn _lwp_setname(target: lwpid_t, name: &str) -> Result<(), Errno> {
    let target = target as usize;
    let name = CString::new(name);
    let name_ptr = name.as_ptr() as usize;
    syscall2(SYS__LWP_SETNAME, target, name_ptr).map(drop)
}

pub unsafe fn _lwp_setprivate(ptr: uintptr_t) -> Result<(), Errno> {
    syscall1(SYS__LWP_SETPRIVATE, ptr).map(drop)
}

pub unsafe fn _lwp_suspend(target: lwpid_t) -> Result<(), Errno> {
    let target = target as usize;
    syscall1(SYS__LWP_SUSPEND, target).map(drop)
}

pub unsafe fn _lwp_wakeup(target: lwpid_t) -> Result<(), Errno> {
    let target = target as usize;
    syscall1(SYS__LWP_WAKEUP, target).map(drop)
}

/// Get a thread's CPU affinity mask.
pub unsafe fn _sched_getaffinity(
    pid: pid_t,
    lid: lwpid_t,
    size: size_t,
    cpuset: &mut cpuset_t,
) -> Result<(), Errno> {
    let pid = pid as usize;
    let lid = lid as usize;
    let cpuset_ptr = cpuset as *mut cpuset_t as usize;
    syscall4(SYS__SCHED_GETAFFINITY, pid, lid, size, cpuset_ptr).map(drop)
}

/// Get scheduling paramters.
pub unsafe fn _sched_getparam(
    pid: pid_t,
    lid: lwpid_t,
    policy: &mut i32,
    param: &mut sched_param_t,
) -> Result<(), Errno> {
    let pid = pid as usize;
    let lid = lid as usize;
    let policy_ptr = policy as *mut i32 as usize;
    let param_ptr = param as *mut sched_param_t as usize;
    syscall4(SYS__SCHED_GETPARAM, pid, lid, policy_ptr, param_ptr).map(drop)
}

pub unsafe fn _sched_protect(priority: i32) -> Result<(), Errno> {
    let priority = priority as usize;
    syscall1(SYS__SCHED_PROTECT, priority).map(drop)
}

/// Set a thread's CPU affinity mask.
pub unsafe fn _sched_setaffinity(
    pid: pid_t,
    lid: lwpid_t,
    size: size_t,
    cpuset: &cpuset_t,
) -> Result<(), Errno> {
    let pid = pid as usize;
    let lid = lid as usize;
    let cpuset_ptr = cpuset as *const cpuset_t as usize;
    syscall4(SYS__SCHED_SETAFFINITY, pid, lid, size, cpuset_ptr).map(drop)
}

/// Set scheduling paramters.
pub unsafe fn _sched_setparam(
    pid: pid_t,
    lid: lwpid_t,
    policy: i32,
    param: &sched_param_t,
) -> Result<(), Errno> {
    let pid = pid as usize;
    let lid = lid as usize;
    let policy = policy as usize;
    let param_ptr = param as *const sched_param_t as usize;
    syscall4(SYS__SCHED_SETPARAM, pid, lid, policy, param_ptr).map(drop)
}

/// Correct the time to allow synchronization of the system clock.
pub unsafe fn __adjtime50(delta: &timeval_t, old_delta: &mut timeval_t) -> Result<(), Errno> {
    let delta_ptr = delta as *const timeval_t as usize;
    let old_delta_ptr = old_delta as *mut timeval_t as usize;
    syscall2(SYS___ADJTIME50, delta_ptr, old_delta_ptr).map(drop)
}

/// Suspend until asynchronous I/O operations or timeout complete (REALTIME)
pub unsafe fn __aio_suspend50(jobs: &[aiocb_t], timeout: &timespec_t) -> Result<(), Errno> {
    let jobs_ptr = jobs.as_ptr() as usize;
    let nent = jobs.len();
    let timeout_ptr = timeout as *const timespec_t as usize;
    syscall3(SYS___AIO_SUSPEND50, jobs_ptr, nent, timeout_ptr).map(drop)
}

/// Get resolution(precision) of the specific clock.
pub unsafe fn __clock_getres50(which_clock: clockid_t, tp: &mut timespec_t) -> Result<(), Errno> {
    let which_clock = which_clock as usize;
    let tp_ptr = tp as *mut timespec_t as usize;
    syscall2(SYS___CLOCK_GETRES50, which_clock, tp_ptr).map(drop)
}

/// Get time of specific clock.
pub unsafe fn __clock_gettime50(which_clock: clockid_t, tp: &mut timespec_t) -> Result<(), Errno> {
    let which_clock = which_clock as usize;
    let tp_ptr = tp as *mut timespec_t as usize;
    syscall2(SYS___CLOCK_GETTIME50, which_clock, tp_ptr).map(drop)
}

/// Set time of specific clock.
pub unsafe fn __clock_settime50(which_clock: clockid_t, tp: &timespec_t) -> Result<(), Errno> {
    let which_clock = which_clock as usize;
    let tp_ptr = tp as *const timespec_t as usize;
    syscall2(SYS___CLOCK_SETTIME50, which_clock, tp_ptr).map(drop)
}

/// Create a child process.
pub unsafe fn __clone(flags: i32, stack: uintptr_t) -> Result<pid_t, Errno> {
    let flags = flags as usize;
    syscall2(SYS___CLONE, flags, stack).map(|ret| ret as pid_t)
}

/// Opens the file referenced by `fh` for reading and/or writing,
/// and returns the file descriptor to the calling process.
pub unsafe fn __fhopen40(fhp: uintptr_t, fh_size: size_t, flags: i32) -> Result<i32, Errno> {
    let flags = flags as usize;
    syscall3(SYS___FHOPEN40, fhp, fh_size, flags).map(|val| val as i32)
}

/// Get file status referenced by `fh`.
pub unsafe fn __fhstat50(fhp: uintptr_t, fh_size: size_t, sb: &mut stat_t) -> Result<(), Errno> {
    let sb_ptr = sb as *mut stat_t as usize;
    syscall3(SYS___FHSTAT50, fhp, fh_size, sb_ptr).map(drop)
}

/// Get filesystem statistics referenced by `fh`.
pub unsafe fn __fhstatvfs140(
    fhp: uintptr_t,
    fh_size: size_t,
    buf: &mut statvfs_t,
    flags: i32,
) -> Result<(), Errno> {
    let buf_ptr = buf as *mut statvfs_t as usize;
    let flags = flags as usize;
    syscall4(SYS___FHSTATVFS140, fhp, fh_size, buf_ptr, flags).map(drop)
}

/// Get file status about a file descriptor.
pub unsafe fn __fstat50(fd: i32, statbuf: &mut stat_t) -> Result<(), Errno> {
    let fd = fd as usize;
    let statbuf_ptr = statbuf as *mut stat_t as usize;
    syscall2(SYS___FSTAT50, fd, statbuf_ptr).map(drop)
}

/// Change timestamp of a file.
pub unsafe fn __futimes50(fd: i32, times: &[timeval_t; 2]) -> Result<(), Errno> {
    let fd = fd as usize;
    let times_ptr = times.as_ptr() as usize;
    syscall2(SYS___FUTIMES50, fd, times_ptr).map(drop)
}

/// Get current working directory.
///
/// # Example
///
/// ```
/// let mut buf = [0_u8; nc::PATH_MAX as usize + 1];
/// let ret = unsafe { nc::__getcwd(buf.as_mut_ptr() as usize, buf.len()) };
/// assert!(ret.is_ok());
/// // Remove null-terminal char.
/// let path_len = ret.unwrap() as usize - 1;
/// let cwd = std::str::from_utf8(&buf[..path_len]);
/// assert!(cwd.is_ok());
/// ```
pub unsafe fn __getcwd(buf: usize, size: size_t) -> Result<ssize_t, Errno> {
    syscall2(SYS___GETCWD, buf, size).map(|ret| ret as ssize_t)
}

/// Get directory entries.
pub unsafe fn getdents(fd: i32, dirp: usize, count: size_t) -> Result<ssize_t, Errno> {
    let fd = fd as usize;
    syscall3(SYS___GETDENTS30, fd, dirp, count).map(|ret| ret as ssize_t)
}

/// Get file handle.
pub unsafe fn __getfh30<P: AsRef<Path>>(
    path: P,
    fhp: uintptr_t,
    fh_size: size_t,
) -> Result<(), Errno> {
    let path = CString::new(path.as_ref());
    let path_ptr = path.as_ptr() as usize;
    syscall3(SYS___GETFH30, path_ptr, fhp, fh_size).map(drop)
}

/// Get value of an interval timer.
pub unsafe fn __getitimer50(which: i32, curr_val: &mut itimerval_t) -> Result<(), Errno> {
    let which = which as usize;
    let curr_val_ptr = curr_val as *mut itimerval_t as usize;
    syscall2(SYS___GETITIMER50, which, curr_val_ptr).map(drop)
}

/// Get login name.
pub unsafe fn __getlogin(name: &mut [u8]) -> Result<(), Errno> {
    // TODO(Shaohua): Convert to CString
    let name_ptr = name.as_mut_ptr() as usize;
    let len = name.len();
    syscall2(SYS___GETLOGIN, name_ptr, len).map(drop)
}

/// Get resource usage.
pub unsafe fn __getrusage50(who: i32, usage: &mut rusage_t) -> Result<(), Errno> {
    let who = who as usize;
    let usage_ptr = usage as *mut rusage_t as usize;
    syscall2(SYS___GETRUSAGE50, who, usage_ptr).map(drop)
}

/// Get time.
pub unsafe fn __gettimeofday590(timeval: &mut timeval_t, tz: &mut timezone_t) -> Result<(), Errno> {
    let timeval_ptr = timeval as *mut timeval_t as usize;
    let tz_ptr = tz as *mut timezone_t as usize;
    syscall2(SYS___GETTIMEOFDAY50, timeval_ptr, tz_ptr).map(drop)
}

/// Get file status about a file, without following symbolic.
pub unsafe fn __lstat50<P: AsRef<Path>>(filename: P, statbuf: &mut stat_t) -> Result<(), Errno> {
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    let statbuf_ptr = statbuf as *mut stat_t as usize;
    syscall2(SYS___LSTAT50, filename_ptr, statbuf_ptr).map(drop)
}

/// Change file last access and modification time.
pub unsafe fn __lutimes50<P: AsRef<Path>>(
    filename: P,
    times: &[timeval_t; 2],
) -> Result<(), Errno> {
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    let times_ptr = times.as_ptr() as usize;
    syscall2(SYS___LUTIMES50, filename_ptr, times_ptr).map(drop)
}

/// Create a special or ordinary file.
pub unsafe fn __mknod50<P: AsRef<Path>>(
    filename: P,
    mode: mode_t,
    dev: dev_t,
) -> Result<(), Errno> {
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    let mode = mode as usize;
    let dev = dev as usize;
    syscall3(SYS___MKNOD50, filename_ptr, mode, dev).map(drop)
}

/// Mount filesystem.
pub unsafe fn mount<P: AsRef<Path>>(
    fs_type: P,
    dir_name: P,
    flags: i32,
    data: usize,
) -> Result<(), Errno> {
    let fs_type = CString::new(fs_type.as_ref());
    let fs_type_ptr = fs_type.as_ptr() as usize;
    let dir_name = CString::new(dir_name.as_ref());
    let dir_name_ptr = dir_name.as_ptr() as usize;
    let flags = flags as usize;
    syscall4(SYS___MOUNT50, fs_type_ptr, dir_name_ptr, flags, data).map(drop)
}

pub unsafe fn __mq_timedreceive50(
    mqdes: mqd_t,
    msg: &mut [u8],
    msg_len: usize,
    msg_prio: &mut u32,
    abs_timeout: &timespec_t,
) -> Result<ssize_t, Errno> {
    let mqdes = mqdes as usize;
    let msg = CString::new(msg);
    let msg_ptr = msg.as_ptr() as usize;
    let msg_prio = msg_prio as *mut u32 as usize;
    let abs_timeout_ptr = abs_timeout as *const timespec_t as usize;
    syscall5(
        SYS___MQ_TIMEDRECEIVE50,
        mqdes,
        msg_ptr,
        msg_len,
        msg_prio,
        abs_timeout_ptr,
    )
    .map(|ret| ret as ssize_t)
}

/// Send message to a message queue.
pub unsafe fn __mq_timedsend50(
    mqdes: mqd_t,
    msg: &[u8],
    msg_len: usize,
    msg_prio: u32,
    abs_timeout: &timespec_t,
) -> Result<(), Errno> {
    let mqdes = mqdes as usize;
    let msg = CString::new(msg);
    let msg_ptr = msg.as_ptr() as usize;
    let msg_prio = msg_prio as usize;
    let abs_timeout_ptr = abs_timeout as *const timespec_t as usize;
    syscall5(
        SYS___MQ_TIMEDSEND50,
        mqdes,
        msg_ptr,
        msg_len,
        msg_prio,
        abs_timeout_ptr,
    )
    .map(drop)
}

/// System V message control operations.
pub unsafe fn __msgctl50(msqid: i32, cmd: i32, buf: &mut msqid_ds_t) -> Result<i32, Errno> {
    let msqid = msqid as usize;
    let cmd = cmd as usize;
    let buf_ptr = buf as *mut msqid_ds_t as usize;
    syscall3(SYS___MSGCTL50, msqid, cmd, buf_ptr).map(|ret| ret as i32)
}

/// Synchronize a file with memory map.
pub unsafe fn __msync13(addr: usize, len: size_t, flags: i32) -> Result<(), Errno> {
    let flags = flags as usize;
    syscall3(SYS___MSYNC13, addr, len, flags).map(drop)
}

/// High resolution sleep.
pub unsafe fn __nanosleep50(req: &timespec_t, rem: Option<&mut timespec_t>) -> Result<(), Errno> {
    let req_ptr = req as *const timespec_t as usize;
    let rem_ptr = rem.map_or(0, |rem| rem as *mut timespec_t as usize);
    syscall2(SYS___NANOSLEEP50, req_ptr, rem_ptr).map(drop)
}

/// Provides the time, maximum error (sync distance) and estimated error (dispersion)
/// to client user application programs.
pub unsafe fn __ntp_gettime50(time: &mut ntptimeval_t) -> Result<i32, Errno> {
    let time_ptr = time as *mut ntptimeval_t as usize;
    syscall1(SYS___NTP_GETTIME50, time_ptr).map(|val| val as i32)
}

/// Change ownership of a file.
pub unsafe fn __posix_chown<P: AsRef<Path>>(
    filename: P,
    user: uid_t,
    group: gid_t,
) -> Result<(), Errno> {
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    let user = user as usize;
    let group = group as usize;
    syscall3(SYS___POSIX_CHOWN, filename_ptr, user, group).map(drop)
}

/// Give advice about use of file data
pub unsafe fn __posix_fadvise50(
    fd: i32,
    pad: i32,
    offset: off_t,
    len: off_t,
    advice: i32,
) -> Result<(), Errno> {
    let fd = fd as usize;
    let pad = pad as usize;
    let offset = offset as usize;
    let len = len as usize;
    let advice = advice as usize;
    syscall5(SYS___POSIX_FADVISE50, fd, pad, offset, len, advice).map(drop)
}

/// Change ownership of a file.
pub unsafe fn __posix_fchown(fd: i32, user: uid_t, group: gid_t) -> Result<(), Errno> {
    let fd = fd as usize;
    let user = user as usize;
    let group = group as usize;
    syscall3(SYS___POSIX_FCHOWN, fd, user, group).map(drop)
}

/// Change ownership of a file. Does not deference symbolic link.
pub unsafe fn __posix_lchown<P: AsRef<Path>>(
    filename: P,
    user: uid_t,
    group: gid_t,
) -> Result<(), Errno> {
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    let user = user as usize;
    let group = group as usize;
    syscall3(SYS___POSIX_LCHOWN, filename_ptr, user, group).map(drop)
}

/// Change name or location of a file.
pub unsafe fn __posix_rename<P: AsRef<Path>>(oldfilename: P, newfilename: P) -> Result<(), Errno> {
    let oldfilename = CString::new(oldfilename.as_ref());
    let oldfilename_ptr = oldfilename.as_ptr() as usize;
    let newfilename = CString::new(newfilename.as_ref());
    let newfilename_ptr = newfilename.as_ptr() as usize;
    syscall2(SYS___POSIX_RENAME, oldfilename_ptr, newfilename_ptr).map(drop)
}

/// Sychronous I/O multiplexing.
///
/// Most architectures can't handle 7-argument syscalls. So we provide a
/// 6-argument version where the sixth argument is a pointer to a structure
/// which has a pointer to the `sigset_t` itself followed by a `size_t` containing
/// the sigset size.
pub unsafe fn pselect6(
    nfds: i32,
    readfds: &mut fd_set_t,
    writefds: &mut fd_set_t,
    exceptfds: &mut fd_set_t,
    timeout: &timespec_t,
    sigmask: &sigset_t,
) -> Result<i32, Errno> {
    let nfds = nfds as usize;
    let readfds_ptr = readfds as *mut fd_set_t as usize;
    let writefds_ptr = writefds as *mut fd_set_t as usize;
    let exceptfds_ptr = exceptfds as *mut fd_set_t as usize;
    let timeout_ptr = timeout as *const timespec_t as usize;
    let sigmask_ptr = sigmask as *const sigset_t as usize;
    syscall6(
        SYS___PSELECT50,
        nfds,
        readfds_ptr,
        writefds_ptr,
        exceptfds_ptr,
        timeout_ptr,
        sigmask_ptr,
    )
    .map(|ret| ret as i32)
}

/// Manipulate disk quotes.
pub unsafe fn __quotactl<P: AsRef<Path>>(path: P, args: &mut quotactl_args_t) -> Result<(), Errno> {
    let path = CString::new(path.as_ref());
    let path_ptr = path.as_ptr() as usize;
    let args_ptr = args as *mut quotactl_args_t as usize;
    syscall2(SYS___QUOTACTL, path_ptr, args_ptr).map(drop)
}

/// Sychronous I/O multiplexing.
pub unsafe fn __select50(
    nfds: i32,
    readfds: &mut fd_set_t,
    writefds: &mut fd_set_t,
    exceptfds: &mut fd_set_t,
    timeout: &mut timeval_t,
) -> Result<i32, Errno> {
    let nfds = nfds as usize;
    let readfds_ptr = readfds as *mut fd_set_t as usize;
    let writefds_ptr = writefds as *mut fd_set_t as usize;
    let exceptfds_ptr = exceptfds as *mut fd_set_t as usize;
    let timeout_ptr = timeout as *mut timeval_t as usize;
    syscall5(
        SYS___SELECT50,
        nfds,
        readfds_ptr,
        writefds_ptr,
        exceptfds_ptr,
        timeout_ptr,
    )
    .map(|ret| ret as i32)
}

/// Set value of an interval timer.
pub unsafe fn __setitimer50(
    which: i32,
    new_val: &itimerval_t,
    old_val: &mut itimerval_t,
) -> Result<(), Errno> {
    let which = which as usize;
    let new_val_ptr = new_val as *const itimerval_t as usize;
    let old_val_ptr = old_val as *mut itimerval_t as usize;
    syscall3(SYS___SETITIMER50, which, new_val_ptr, old_val_ptr).map(drop)
}

/// Set login name.
pub unsafe fn __setlogin(name: &str) -> Result<(), Errno> {
    let name = CString::new(name);
    let name_ptr = name.as_ptr() as usize;
    syscall1(SYS___SETLOGIN, name_ptr).map(drop)
}

/// Set system time and timezone.
pub unsafe fn __settimeofday50(timeval: &timeval_t, tz: &timezone_t) -> Result<(), Errno> {
    let timeval_ptr = timeval as *const timeval_t as usize;
    let tz_ptr = tz as *const timezone_t as usize;
    syscall2(SYS___SETTIMEOFDAY50, timeval_ptr, tz_ptr).map(drop)
}

/// System V shared memory control.
pub unsafe fn __shmctl50(shmid: i32, cmd: i32, buf: &mut shmid_ds_t) -> Result<i32, Errno> {
    let shmid = shmid as usize;
    let cmd = cmd as usize;
    let buf_ptr = buf as *mut shmid_ds_t as usize;
    syscall3(SYS___SHMCTL50, shmid, cmd, buf_ptr).map(|ret| ret as i32)
}

/// Get/set signal stack context.
pub unsafe fn __sigaltstack14(uss: &sigaltstack_t, uoss: &mut sigaltstack_t) -> Result<(), Errno> {
    let uss_ptr = uss as *const sigaltstack_t as usize;
    let uoss_ptr = uoss as *mut sigaltstack_t as usize;
    syscall2(SYS___SIGALTSTACK14, uss_ptr, uoss_ptr).map(drop)
}

/// Examine pending signals.
pub unsafe fn __sigpending14(set: &mut sigset_t) -> Result<(), Errno> {
    let set_ptr = set as *mut sigset_t as usize;
    syscall1(SYS___SIGPENDING14, set_ptr).map(drop)
}

/// Examine and change blocked signals.
pub unsafe fn __sigprocmask14(
    how: i32,
    newset: &mut sigset_t,
    oldset: &mut sigset_t,
) -> Result<(), Errno> {
    let how = how as usize;
    let newset_ptr = newset as *mut sigset_t as usize;
    let oldset_ptr = oldset as *mut sigset_t as usize;
    syscall3(SYS___SIGPROCMASK14, how, newset_ptr, oldset_ptr).map(drop)
}

/// Wait for a signal.
pub unsafe fn __sigsuspend14(mask: &sigset_t) -> Result<(), Errno> {
    let mask_ptr = mask as *const sigset_t as usize;
    syscall1(SYS___SIGSUSPEND14, mask_ptr).map(drop)
}

/// Create an endpoint for communication.
pub unsafe fn __socket30(domain: i32, sock_type: i32, protocol: i32) -> Result<i32, Errno> {
    let domain = domain as usize;
    let sock_type = sock_type as usize;
    let protocol = protocol as usize;
    syscall3(SYS___SOCKET30, domain, sock_type, protocol).map(|ret| ret as i32)
}

/// Get file status about a file.
pub unsafe fn __stat50<P: AsRef<Path>>(filename: P, statbuf: &mut stat_t) -> Result<(), Errno> {
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    let statbuf_ptr = statbuf as *mut stat_t as usize;
    syscall2(SYS___STAT50, filename_ptr, statbuf_ptr).map(drop)
}

/// Read/write system parameters.
pub unsafe fn __sysctl(
    name: &i32,
    name_len: u32,
    old_val: usize,
    old_len: size_t,
    new_val: usize,
    new_len: size_t,
) -> Result<(), Errno> {
    let name_ptr = name as *const i32 as usize;
    let name_len = name_len as usize;
    syscall6(
        SYS___SYSCTL,
        name_ptr,
        name_len,
        old_val,
        old_len,
        new_val,
        new_len,
    )
    .map(drop)
}

/// Fetch state of per-process timer>
pub unsafe fn __timer_gettime50(timer_id: timer_t, curr: &mut itimerspec_t) -> Result<(), Errno> {
    let timer_id = timer_id as usize;
    let curr_ptr = curr as *mut itimerspec_t as usize;
    syscall2(SYS___TIMER_GETTIME50, timer_id, curr_ptr).map(drop)
}

/// Arm/disarm state of per-process timer.
pub unsafe fn __timer_settime50(
    timer_id: timer_t,
    flags: i32,
    new_value: &itimerspec_t,
    old_value: Option<&mut itimerspec_t>,
) -> Result<(), Errno> {
    let timer_id = timer_id as usize;
    let flags = flags as usize;
    let new_value_ptr = new_value as *const itimerspec_t as usize;
    let old_value_ptr = old_value.map_or(0, |old_value| old_value as *mut itimerspec_t as usize);
    syscall4(
        SYS___TIMER_SETTIME50,
        timer_id,
        flags,
        new_value_ptr,
        old_value_ptr,
    )
    .map(drop)
}

/// Change file last access and modification time.
pub unsafe fn __utimes50<P: AsRef<Path>>(filename: P, times: &[timeval_t; 2]) -> Result<(), Errno> {
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    let times_ptr = times.as_ptr() as usize;
    syscall2(SYS___UTIMES50, filename_ptr, times_ptr).map(drop)
}

/// Create a child process and wait until it is terminated.
pub unsafe fn __vfork14() -> Result<pid_t, Errno> {
    syscall0(SYS___VFORK14).map(|ret| ret as pid_t)
}

/// Wait for process to change state.
pub unsafe fn __wait450(
    pid: pid_t,
    wstatus: &mut i32,
    options: i32,
    rusage: &mut rusage_t,
) -> Result<pid_t, Errno> {
    let pid = pid as usize;
    let wstatus_ptr = wstatus as *mut i32 as usize;
    let options = options as usize;
    let rusage_ptr = rusage as *mut rusage_t as usize;
    syscall4(SYS___WAIT450, pid, wstatus_ptr, options, rusage_ptr).map(|ret| ret as pid_t)
}
