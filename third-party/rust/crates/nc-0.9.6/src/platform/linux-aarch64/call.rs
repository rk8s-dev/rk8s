// Copyright (c) 2024 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
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
use alloc::vec::Vec;

use crate::c_str::CString;
use crate::path::Path;
use crate::syscalls::*;
use crate::sysno::*;
use crate::types::*;

/// Accept a connection on a socket.
///
/// On success, `accept()` return a file descriptor for the accepted socket.
///
/// # Examples
///
/// ```
/// use nc::Errno;
/// use std::mem::{size_of, transmute};
/// use std::thread;
///
/// const SERVER_PORT: u16 = 18082;
///
/// #[must_use]
/// #[inline]
/// const fn htons(host: u16) -> u16 {
///     host.to_be()
/// }
///
/// fn main() -> Result<(), Errno> {
///     let listen_fd = unsafe { nc::socket(nc::AF_INET, nc::SOCK_STREAM, 0)? };
///     println!("listen fd: {listen_fd}");
///
///     let addr = nc::sockaddr_in_t {
///         sin_family: nc::AF_INET as nc::sa_family_t,
///         sin_port: htons(SERVER_PORT),
///         sin_addr: nc::in_addr_t {
///             s_addr: nc::INADDR_ANY as u32,
///         },
///         ..Default::default()
///     };
///     println!("addr: {addr:?}");
///
///     let ret = unsafe {
///         let addr_alias = transmute::<&nc::sockaddr_in_t, &nc::sockaddr_t>(&addr);
///         nc::bind(listen_fd, addr_alias, size_of::<nc::sockaddr_in_t>() as u32)
///     };
///     assert!(ret.is_ok());
///
///     // Start worker thread
///     thread::spawn(|| {
///         println!("worker thread started");
///         let socket_fd = unsafe { nc::socket(nc::AF_INET, nc::SOCK_STREAM, 0) };
///         assert!(socket_fd.is_ok());
///         if let Ok(socket_fd) = socket_fd {
///             let addr = nc::sockaddr_in_t {
///                 sin_family: nc::AF_INET as nc::sa_family_t,
///                 sin_port: htons(SERVER_PORT),
///                 sin_addr: nc::in_addr_t {
///                     s_addr: nc::INADDR_ANY as u32,
///                 },
///                 ..Default::default()
///             };
///             unsafe {
///                 let addr_alias = transmute::<&nc::sockaddr_in_t, &nc::sockaddr_t>(&addr);
///                 let ret = nc::connect(socket_fd, addr_alias, size_of::<nc::sockaddr_in_t>() as u32);
///                 assert_eq!(ret, Ok(()));
///             }
///         } else {
///             eprintln!("Failed to create socket");
///         }
///     });
///
///     unsafe {
///         nc::listen(listen_fd, nc::SOCK_STREAM)?;
///     }
///
///     let mut conn_addr = nc::sockaddr_in_t::default();
///     let mut conn_addr_len: nc::socklen_t = 0;
///     let conn_fd = unsafe {
///         nc::accept(
///             listen_fd,
///             Some(&mut conn_addr as *mut nc::sockaddr_in_t as *mut nc::sockaddr_t),
///             Some(&mut conn_addr_len),
///         )?
///     };
///     println!("conn_fd: {conn_fd}");
///
///     unsafe {
///         nc::close(listen_fd)?;
///     }
///
///     Ok(())
/// }
/// ```
pub unsafe fn accept(
    sockfd: i32,
    addr: Option<*mut sockaddr_t>,
    addrlen: Option<&mut socklen_t>,
) -> Result<i32, Errno> {
    let sockfd = sockfd as usize;
    let addr_ptr = addr.map_or(core::ptr::null_mut::<sockaddr_t>() as usize, |addr| {
        addr as usize
    });
    let addrlen_ptr = addrlen.map_or(core::ptr::null_mut::<socklen_t>() as usize, |addrlen| {
        addrlen as *mut socklen_t as usize
    });
    syscall3(SYS_ACCEPT, sockfd, addr_ptr, addrlen_ptr).map(|val| val as i32)
}

/// Accept a connection on a socket.
///
/// # Examples
///
/// ```
/// use nc::Errno;
/// use std::mem::{size_of, transmute};
/// use std::thread;
///
/// const SERVER_PORT: u16 = 18083;
///
/// #[must_use]
/// #[inline]
/// const fn htons(host: u16) -> u16 {
///     host.to_be()
/// }
///
/// fn main() -> Result<(), Errno> {
///     let listen_fd = unsafe { nc::socket(nc::AF_INET, nc::SOCK_STREAM, 0)? };
///     println!("listen fd: {listen_fd}");
///
///     let addr = nc::sockaddr_in_t {
///         sin_family: nc::AF_INET as nc::sa_family_t,
///         sin_port: htons(SERVER_PORT),
///         sin_addr: nc::in_addr_t {
///             s_addr: nc::INADDR_ANY as u32,
///         },
///         ..Default::default()
///     };
///     println!("addr: {addr:?}");
///
///     let ret = unsafe {
///         let addr_alias = transmute::<&nc::sockaddr_in_t, &nc::sockaddr_t>(&addr);
///         nc::bind(listen_fd, addr_alias, size_of::<nc::sockaddr_in_t>() as u32)
///     };
///     assert!(ret.is_ok());
///
///     // Start worker thread
///     thread::spawn(|| {
///         println!("worker thread started");
///         let socket_fd = unsafe { nc::socket(nc::AF_INET, nc::SOCK_STREAM, 0) };
///         assert!(socket_fd.is_ok());
///         if let Ok(socket_fd) = socket_fd {
///             let addr = nc::sockaddr_in_t {
///                 sin_family: nc::AF_INET as nc::sa_family_t,
///                 sin_port: htons(SERVER_PORT),
///                 sin_addr: nc::in_addr_t {
///                     s_addr: nc::INADDR_ANY as u32,
///                 },
///                 ..Default::default()
///             };
///             unsafe {
///                 let addr_alias = transmute::<&nc::sockaddr_in_t, &nc::sockaddr_t>(&addr);
///                 let ret = nc::connect(socket_fd, addr_alias, size_of::<nc::sockaddr_in_t>() as u32);
///                 assert_eq!(ret, Ok(()));
///             }
///         } else {
///             eprintln!("Failed to create socket");
///         }
///     });
///
///     unsafe {
///         nc::listen(listen_fd, nc::SOCK_STREAM)?;
///     }
///
///     let mut conn_addr = nc::sockaddr_in_t::default();
///     let mut conn_addr_len: nc::socklen_t = 0;
///     let conn_fd = unsafe {
///         nc::accept4(
///             listen_fd,
///             Some(&mut conn_addr as *mut nc::sockaddr_in_t as *mut nc::sockaddr_t),
///             Some(&mut conn_addr_len),
///             nc::SOCK_CLOEXEC,
///         )?
///     };
///     println!("conn_fd: {conn_fd}");
///
///     unsafe {
///         nc::close(listen_fd)?;
///     }
///
///     Ok(())
/// }
/// ```
pub unsafe fn accept4(
    sockfd: i32,
    addr: Option<*mut sockaddr_t>,
    addrlen: Option<&mut socklen_t>,
    flags: i32,
) -> Result<i32, Errno> {
    let sockfd = sockfd as usize;
    let addr_ptr = addr.map_or(core::ptr::null_mut::<sockaddr_t>() as usize, |addr| {
        addr as usize
    });
    let addrlen_ptr = addrlen.map_or(core::ptr::null_mut::<socklen_t>() as usize, |addrlen| {
        addrlen as *mut socklen_t as usize
    });
    let flags = flags as usize;
    syscall4(SYS_ACCEPT4, sockfd, addr_ptr, addrlen_ptr, flags).map(|val| val as i32)
}

/// Switch process accounting.
///
/// # Examples
///
/// ```
/// let path = "/tmp/nc-acct";
/// let fd = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_WRONLY | nc::O_CREAT, 0o644) };
/// assert!(fd.is_ok());
/// let fd = fd.unwrap();
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::acct(Some(path)) };
/// assert_eq!(ret, Err(nc::EPERM));
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, path, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn acct<P: AsRef<Path>>(filename: Option<P>) -> Result<(), Errno> {
    let filename = filename.map(|filename| CString::new(filename.as_ref()));
    let filename_ptr = filename.map_or(core::ptr::null::<u8>() as usize, |filename| {
        filename.as_ptr() as usize
    });
    syscall1(SYS_ACCT, filename_ptr).map(drop)
}

/// Add a key to the kernel's key management facility.
pub unsafe fn add_key<P: AsRef<Path>>(
    type_: P,
    description: P,
    payload: *const core::ffi::c_void,
    plen: size_t,
    dest_keyring: key_serial_t,
) -> Result<key_serial_t, Errno> {
    let type_ = CString::new(type_.as_ref());
    let type_ptr = type_.as_ptr() as usize;
    let payload = payload as usize;
    let description = CString::new(description.as_ref());
    let description_ptr = description.as_ptr() as usize;
    let dest_keyring = dest_keyring as usize;
    syscall5(
        SYS_ADD_KEY,
        type_ptr,
        description_ptr,
        payload,
        plen,
        dest_keyring,
    )
    .map(|ret| ret as key_serial_t)
}

/// Tune kernel clock. Returns clock state on success.
///
/// # Examples
///
/// ```
/// let mut tm = nc::timex_t::default();
/// let ret = unsafe { nc::adjtimex(&mut tm) };
/// assert!(ret.is_ok());
/// assert!(tm.time.tv_sec > 1611552896);
/// ```
pub unsafe fn adjtimex(buf: &mut timex_t) -> Result<i32, Errno> {
    let buf_ptr = buf as *mut timex_t as usize;
    syscall1(SYS_ADJTIMEX, buf_ptr).map(|ret| ret as i32)
}

/// Bind a name to a socket.
///
/// # Examples
///
/// ```
/// use nc::Errno;
/// use std::mem::{size_of, transmute};
/// use std::thread;
///
/// const SERVER_PORT: u16 = 18085;
///
/// #[must_use]
/// #[inline]
/// const fn htons(host: u16) -> u16 {
///     host.to_be()
/// }
///
/// fn main() -> Result<(), Errno> {
///     let listen_fd = unsafe { nc::socket(nc::AF_INET, nc::SOCK_STREAM, 0)? };
///     println!("listen fd: {listen_fd}");
///
///     let addr = nc::sockaddr_in_t {
///         sin_family: nc::AF_INET as nc::sa_family_t,
///         sin_port: htons(SERVER_PORT),
///         sin_addr: nc::in_addr_t {
///             s_addr: nc::INADDR_ANY as u32,
///         },
///         ..Default::default()
///     };
///     println!("addr: {addr:?}");
///
///     let ret = unsafe {
///         let addr_alias = transmute::<&nc::sockaddr_in_t, &nc::sockaddr_t>(&addr);
///         nc::bind(listen_fd, addr_alias, size_of::<nc::sockaddr_in_t>() as u32)
///     };
///     assert!(ret.is_ok());
///
///     // Start worker thread
///     thread::spawn(|| {
///         println!("worker thread started");
///         let socket_fd = unsafe { nc::socket(nc::AF_INET, nc::SOCK_STREAM, 0) };
///         assert!(socket_fd.is_ok());
///         if let Ok(socket_fd) = socket_fd {
///             let addr = nc::sockaddr_in_t {
///                 sin_family: nc::AF_INET as nc::sa_family_t,
///                 sin_port: htons(SERVER_PORT),
///                 sin_addr: nc::in_addr_t {
///                     s_addr: nc::INADDR_ANY as u32,
///                 },
///                 ..Default::default()
///             };
///             unsafe {
///                 let addr_alias = transmute::<&nc::sockaddr_in_t, &nc::sockaddr_t>(&addr);
///                 let ret = nc::connect(socket_fd, addr_alias, size_of::<nc::sockaddr_in_t>() as u32);
///                 assert_eq!(ret, Ok(()));
///             }
///         } else {
///             eprintln!("Failed to create socket");
///         }
///     });
///
///     unsafe {
///         nc::listen(listen_fd, nc::SOCK_STREAM)?;
///     }
///
///     let conn_fd = unsafe {
///         nc::accept4(
///             listen_fd,
///             None,
///             None,
///             nc::SOCK_CLOEXEC,
///         )?
///     };
///     println!("conn_fd: {conn_fd}");
///
///     unsafe {
///         nc::close(listen_fd)?;
///     }
///
///     Ok(())
/// }
/// ```
pub unsafe fn bind(sockfd: i32, addr: *const sockaddr_t, addrlen: socklen_t) -> Result<(), Errno> {
    let sockfd = sockfd as usize;
    let addr_ptr = addr as usize;
    let addrlen = addrlen as usize;
    syscall3(SYS_BIND, sockfd, addr_ptr, addrlen).map(drop)
}

/// Perform a command on an extended BPF map or program
pub unsafe fn bpf(cmd: i32, attr: &mut bpf_attr_t, size: u32) -> Result<i32, Errno> {
    let cmd = cmd as usize;
    let attr_ptr = attr as *mut bpf_attr_t as usize;
    let size = size as usize;
    syscall3(SYS_BPF, cmd, attr_ptr, size).map(|ret| ret as i32)
}

/// Change data segment size.
pub unsafe fn brk(addr: usize) -> Result<(), Errno> {
    syscall1(SYS_BRK, addr).map(drop)
}

/// `cachestat()` returns the page cache statistics of a file in the
/// bytes range specified by `off` and `len`.
///
/// Number of cached pages, number of dirty pages, number of pages marked for writeback,
/// number of evicted pages, and number of recently evicted pages.
///
/// An evicted page is a page that is previously in the page cache
/// but has been evicted since. A page is recently evicted if its last
/// eviction was recent enough that its reentry to the cache would
/// indicate that it is actively being used by the system, and that
/// there is memory pressure on the system.
///
/// `off` and `len` must be non-negative integers. If `len` > 0,
/// the queried range is [`off`, `off` + `len`]. If `len` == 0,
/// we will query in the range from `off` to the end of the file.
///
/// The `flags` argument is unused for now, but is included for future
/// extensibility. User should pass 0 (i.e no flag specified).
///
/// Currently, hugetlbfs is not supported.
///
/// Because the status of a page can change after `cachestat()` checks it
/// but before it returns to the application, the returned values may
/// contain stale information.
///
/// return values:
/// - zero       - success
/// - EFAULT     - cstat or `cstat_range` points to an illegal address
/// - EINVAL     - invalid flags
/// - EBADF      - invalid file descriptor
/// - EOPNOTSUPP - file descriptor is of a hugetlbfs file
pub unsafe fn cachestat(
    fd: u32,
    cstat_range: &mut cachestat_range_t,
    cstat: &mut cachestat_t,
    flags: u32,
) -> Result<(), Errno> {
    let fd = fd as usize;
    let cstat_range_ptr = cstat_range as *mut cachestat_range_t as usize;
    let cstat_ptr = cstat as *mut cachestat_t as usize;
    let flags = flags as usize;
    syscall4(SYS_CACHESTAT, fd, cstat_range_ptr, cstat_ptr, flags).map(drop)
}

/// Get capabilities of thread.
pub unsafe fn capget(
    hdrp: &mut cap_user_header_t,
    data: &mut cap_user_data_t,
) -> Result<(), Errno> {
    let hdrp_ptr = hdrp as *mut cap_user_header_t as usize;
    let data_ptr = data as *mut cap_user_data_t as usize;
    syscall2(SYS_CAPGET, hdrp_ptr, data_ptr).map(drop)
}

/// Set capabilities of thread.
pub unsafe fn capset(hdrp: &mut cap_user_header_t, data: &cap_user_data_t) -> Result<(), Errno> {
    let hdrp_ptr = hdrp as *mut cap_user_header_t as usize;
    let data_ptr = data as *const cap_user_data_t as usize;
    syscall2(SYS_CAPSET, hdrp_ptr, data_ptr).map(drop)
}

/// Change working directory.
///
/// # Examples
///
/// ```
/// let path = "/tmp";
/// // Open folder directly.
/// let ret = unsafe { nc::chdir(path) };
/// assert!(ret.is_ok());
///
/// let mut buf = [0_u8; nc::PATH_MAX as usize + 1];
/// let ret = unsafe { nc::getcwd(&mut buf) };
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

/// Change the root directory.
///
/// # Examples
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

/// Tune kernel clock.
///
/// Returns clock state on success.
///
/// # Examples
///
/// ```
/// let mut tm = nc::timex_t::default();
/// let ret = unsafe { nc::clock_adjtime(nc::CLOCK_REALTIME, &mut tm) };
/// assert!(ret.is_ok());
/// assert!(tm.time.tv_sec > 1611552896);
/// ```
pub unsafe fn clock_adjtime(which_clock: clockid_t, tx: &mut timex_t) -> Result<(), Errno> {
    let which_clock = which_clock as usize;
    let tx_ptr = tx as *mut timex_t as usize;
    syscall2(SYS_CLOCK_ADJTIME, which_clock, tx_ptr).map(drop)
}

/// Get resolution(precision) of the specific clock.
///
/// # Examples
///
/// ```
/// let mut tp = nc::timespec_t::default();
/// let ret = unsafe { nc::clock_getres(nc::CLOCK_BOOTTIME, Some(&mut tp)) };
/// assert!(ret.is_ok());
/// assert!(tp.tv_nsec > 0);
/// ```
pub unsafe fn clock_getres(
    which_clock: clockid_t,
    tp: Option<&mut timespec_t>,
) -> Result<(), Errno> {
    let which_clock = which_clock as usize;
    let tp_ptr = tp.map_or(core::ptr::null_mut::<timespec_t>() as usize, |tp| {
        tp as *mut timespec_t as usize
    });
    syscall2(SYS_CLOCK_GETRES, which_clock, tp_ptr).map(drop)
}

/// Get time of specific clock.
///
/// # Examples
///
/// ```
/// let mut tp = nc::timespec_t::default();
/// let ret = unsafe { nc::clock_gettime(nc::CLOCK_REALTIME_COARSE, &mut tp) };
/// assert!(ret.is_ok());
/// assert!(tp.tv_sec > 0);
/// ```
pub unsafe fn clock_gettime(which_clock: clockid_t, tp: &mut timespec_t) -> Result<(), Errno> {
    let which_clock = which_clock as usize;
    let tp_ptr = tp as *mut timespec_t as usize;
    syscall2(SYS_CLOCK_GETTIME, which_clock, tp_ptr).map(drop)
}

/// High resolution sleep with a specific clock.
///
/// # Examples
///
/// ```
/// let t = nc::timespec_t {
///     tv_sec: 1,
///     tv_nsec: 0,
/// };
/// let ret = unsafe { nc::clock_nanosleep(nc::CLOCK_MONOTONIC, 0, &t, None) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn clock_nanosleep(
    which_clock: clockid_t,
    flags: i32,
    request: &timespec_t,
    remain: Option<&mut timespec_t>,
) -> Result<(), Errno> {
    let which_clock = which_clock as usize;
    let flags = flags as usize;
    let request_ptr = request as *const timespec_t as usize;
    let remain_ptr = remain.map_or(core::ptr::null_mut::<timespec_t>() as usize, |remain| {
        remain as *mut timespec_t as usize
    });
    syscall4(
        SYS_CLOCK_NANOSLEEP,
        which_clock,
        flags,
        request_ptr,
        remain_ptr,
    )
    .map(drop)
}

/// Set time of specific clock.
///
/// # Examples
///
/// ```
/// let mut tp = nc::timespec_t::default();
/// let ret = unsafe { nc::clock_gettime(nc::CLOCK_REALTIME, &mut tp) };
/// assert!(ret.is_ok());
/// assert!(tp.tv_sec > 0);
/// let ret = unsafe { nc::clock_settime(nc::CLOCK_REALTIME, &tp) };
/// assert!(ret.is_err());
/// assert_eq!(ret, Err(nc::EPERM));
/// ```
pub unsafe fn clock_settime(which_clock: clockid_t, tp: &timespec_t) -> Result<(), Errno> {
    let which_clock = which_clock as usize;
    let tp_ptr = tp as *const timespec_t as usize;
    syscall2(SYS_CLOCK_SETTIME, which_clock, tp_ptr).map(drop)
}

/// Create a child process.
pub unsafe fn clone(
    clone_flags: usize,
    child_stack: *const core::ffi::c_void,
    parent_tid: Option<&mut pid_t>,
    tls: Option<*const core::ffi::c_void>,
    child_tid: Option<&mut pid_t>,
) -> Result<pid_t, Errno> {
    use core::ptr::null_mut;
    let child_stack = child_stack as usize;
    let parent_tid_ptr = parent_tid.map_or(null_mut::<pid_t>() as usize, |parent_tid| {
        parent_tid as *mut pid_t as usize
    });
    let child_tid_ptr = child_tid.map_or(null_mut::<pid_t>() as usize, |child_tid| {
        child_tid as *mut pid_t as usize
    });
    let tls_ptr = tls.map_or(core::ptr::null::<u8>() as usize, |tls| tls as usize);
    syscall5(
        SYS_CLONE,
        clone_flags,
        child_stack,
        parent_tid_ptr,
        tls_ptr,
        child_tid_ptr,
    )
    .map(|ret| ret as pid_t)
}

/// New api to create child process.
///
/// # Examples
///
/// ```
/// let mut args = nc::clone_args_t::default();
/// let mut pid_fd: i32 = -1;
/// args.exit_signal = nc::SIGCHLD as u64;
/// args.pidfd = &mut pid_fd as *mut i32 as usize as u64;
/// args.flags = nc::CLONE_PIDFD as u64 | nc::CLONE_PARENT_SETTID as u64;
/// let pid = unsafe { nc::clone3(&args) };
/// assert!(pid.is_ok());
/// ```
pub unsafe fn clone3(cl_args: &clone_args_t) -> Result<pid_t, Errno> {
    let cl_args_ptr = cl_args as *const clone_args_t as usize;
    let size = core::mem::size_of::<clone_args_t>();
    syscall2(SYS_CLONE3, cl_args_ptr, size).map(|ret| ret as pid_t)
}

/// Close a file descriptor.
///
/// # Examples
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

/// Close all file descriptors in a given range
///
/// Parameters:
/// - `fd`: starting file descriptor to close
/// - `max_fd`: last file descriptor to close
/// - `flags`: reserved for future extensions
///
/// # Examples
///
/// ```
/// const STDOUT_FD: u32 = 1;
/// const STDERR_FD: u32 = 2;
/// let ret = unsafe { nc::close_range(STDOUT_FD, STDERR_FD, nc::CLOSE_RANGE_CLOEXEC) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn close_range(fd: u32, max_fd: u32, flags: u32) -> Result<(), Errno> {
    let fd = fd as usize;
    let max_fd = max_fd as usize;
    let flags = flags as usize;
    syscall3(SYS_CLOSE_RANGE, fd, max_fd, flags).map(drop)
}

/// Initialize a connection on a socket.
///
/// # Examples
///
/// ```
/// use nc::Errno;
/// use std::mem::{size_of, transmute};
/// use std::thread;
///
/// const SERVER_PORT: u16 = 18081;
///
/// #[must_use]
/// #[inline]
/// const fn htons(host: u16) -> u16 {
///     host.to_be()
/// }
///
/// fn main() -> Result<(), Errno> {
///     let listen_fd = unsafe { nc::socket(nc::AF_INET, nc::SOCK_STREAM, 0)? };
///     println!("listen fd: {listen_fd}");
///
///     let addr = nc::sockaddr_in_t {
///         sin_family: nc::AF_INET as nc::sa_family_t,
///         sin_port: htons(SERVER_PORT),
///         sin_addr: nc::in_addr_t {
///             s_addr: nc::INADDR_ANY as u32,
///         },
///         ..Default::default()
///     };
///     println!("addr: {addr:?}");
///
///     let ret = unsafe {
///         let addr_alias = transmute::<&nc::sockaddr_in_t, &nc::sockaddr_t>(&addr);
///         nc::bind(listen_fd, addr_alias, size_of::<nc::sockaddr_in_t>() as u32)
///     };
///     assert!(ret.is_ok());
///
///     // Start worker thread
///     thread::spawn(|| {
///         println!("worker thread started");
///         let socket_fd = unsafe { nc::socket(nc::AF_INET, nc::SOCK_STREAM, 0) };
///         assert!(socket_fd.is_ok());
///         if let Ok(socket_fd) = socket_fd {
///             let addr = nc::sockaddr_in_t {
///                 sin_family: nc::AF_INET as nc::sa_family_t,
///                 sin_port: htons(SERVER_PORT),
///                 sin_addr: nc::in_addr_t {
///                     s_addr: nc::INADDR_ANY as u32,
///                 },
///                 ..Default::default()
///             };
///             unsafe {
///                 let addr_alias = transmute::<&nc::sockaddr_in_t, &nc::sockaddr_t>(&addr);
///                 let ret = nc::connect(socket_fd, addr_alias, size_of::<nc::sockaddr_in_t>() as u32);
///                 assert_eq!(ret, Ok(()));
///             }
///         } else {
///             eprintln!("Failed to create socket");
///         }
///     });
///
///     unsafe {
///         nc::listen(listen_fd, nc::SOCK_STREAM)?;
///     }
///
///     let mut conn_addr = nc::sockaddr_in_t::default();
///     let mut conn_addr_len: nc::socklen_t = 0;
///     let conn_fd = unsafe {
///         nc::accept4(
///             listen_fd,
///             Some(&mut conn_addr as *mut nc::sockaddr_in_t as *mut nc::sockaddr_t),
///             Some(&mut conn_addr_len),
///             nc::SOCK_CLOEXEC,
///         )?
///     };
///     println!("conn_fd: {conn_fd}");
///
///     unsafe {
///         nc::close(listen_fd)?;
///     }
///
///     Ok(())
/// }
/// ```
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

/// Copy a range of data from one file to another.
///
/// # Examples
///
/// ```
/// let path_in = "/tmp/nc-copy-file-range.in";
/// let fd_in = unsafe { nc::openat(nc::AT_FDCWD, path_in, nc::O_RDWR | nc::O_CREAT, 0o644) };
/// assert!(fd_in.is_ok());
/// let fd_in = fd_in.unwrap();
/// let msg = b"Hello, rust";
/// let ret = unsafe { nc::write(fd_in, msg) };
/// assert_eq!(ret, Ok(msg.len() as nc::ssize_t));
/// let path_out = "/tmp/nc-copy-file-range.out";
/// let fd_out = unsafe { nc::openat(nc::AT_FDCWD, path_out, nc::O_WRONLY | nc::O_CREAT, 0o644) };
/// assert!(fd_out.is_ok());
/// let fd_out = fd_out.unwrap();
/// let mut off_in = 0;
/// let mut off_out = 0;
/// let copy_len = 64;
/// let ret = unsafe { nc::copy_file_range(fd_in, &mut off_in, fd_out, &mut off_out, copy_len, 0) };
/// println!("ret: {ret:?}");
/// assert!(ret.is_ok());
/// assert_eq!(ret, Ok(msg.len() as nc::ssize_t));
/// let ret = unsafe { nc::close(fd_in) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::close(fd_out) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, path_out, 0) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, path_in, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn copy_file_range(
    fd_in: i32,
    off_in: &mut loff_t,
    fd_out: i32,
    off_out: &mut loff_t,
    len: size_t,
    flags: u32,
) -> Result<ssize_t, Errno> {
    let fd_in = fd_in as usize;
    let off_in_ptr = off_in as *mut loff_t as usize;
    let fd_out = fd_out as usize;
    let off_out_ptr = off_out as *mut loff_t as usize;
    let flags = flags as usize;
    syscall6(
        SYS_COPY_FILE_RANGE,
        fd_in,
        off_in_ptr,
        fd_out,
        off_out_ptr,
        len,
        flags,
    )
    .map(|ret| ret as ssize_t)
}

/// Unlock a kernel module.
pub unsafe fn delete_module<P: AsRef<Path>>(name: P, flags: u32) -> Result<(), Errno> {
    let name = CString::new(name.as_ref());
    let name_ptr = name.as_ptr() as usize;
    let flags = flags as usize;
    syscall2(SYS_DELETE_MODULE, name_ptr, flags).map(drop)
}

/// Create a copy of the file descriptor `oldfd`, using the lowest available
/// file descriptor.
///
/// # Examples
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

/// Save as `dup2()`, but can set the close-on-exec flag on `newfd`.
///
/// # Examples
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

/// Open an epoll file descriptor.
///
/// # Examples
///
/// ```
/// let poll_fd = unsafe { nc::epoll_create1(nc::EPOLL_CLOEXEC) };
/// assert!(poll_fd.is_ok());
/// let poll_fd = poll_fd.unwrap();
/// let ret = unsafe { nc::close(poll_fd) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn epoll_create1(flags: i32) -> Result<i32, Errno> {
    let flags = flags as usize;
    syscall1(SYS_EPOLL_CREATE1, flags).map(|ret| ret as i32)
}

/// Control interface for an epoll file descriptor.
///
/// # Examples
///
/// ```
/// let epfd = unsafe { nc::epoll_create1(nc::EPOLL_CLOEXEC) };
/// assert!(epfd.is_ok());
/// let epfd = epfd.unwrap();
/// let mut fds: [i32; 2] = [0, 0];
/// let ret = unsafe { nc::pipe2(&mut fds, 0) };
/// assert!(ret.is_ok());
/// let mut event = nc::epoll_event_t::default();
/// event.events = nc::EPOLLIN | nc::EPOLLET;
/// event.data.fd = fds[0];
/// let ctl_ret = unsafe { nc::epoll_ctl(epfd, nc::EPOLL_CTL_ADD, fds[0], &mut event) };
/// assert!(ctl_ret.is_ok());
/// let ret = unsafe { nc::close(fds[0]) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::close(fds[1]) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::close(epfd) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn epoll_ctl(
    epfd: i32,
    op: i32,
    fd: i32,
    event: &mut epoll_event_t,
) -> Result<(), Errno> {
    let epfd = epfd as usize;
    let op = op as usize;
    let fd = fd as usize;
    let event_ptr = event as *mut epoll_event_t as usize;
    syscall4(SYS_EPOLL_CTL, epfd, op, fd, event_ptr).map(drop)
}

/// Wait for an I/O event on an epoll file descriptor.
///
/// # Examples
///
/// ```
/// let epfd = unsafe { nc::epoll_create1(nc::EPOLL_CLOEXEC) };
/// assert!(epfd.is_ok());
/// let epfd = epfd.unwrap();
/// let mut fds: [i32; 2] = [0, 0];
/// let ret = unsafe { nc::pipe2(&mut fds, 0) };
/// assert!(ret.is_ok());
/// let mut event = nc::epoll_event_t::default();
/// event.events = nc::EPOLLIN | nc::EPOLLET;
/// event.data.fd = fds[0];
/// let ctl_ret = unsafe { nc::epoll_ctl(epfd, nc::EPOLL_CTL_ADD, fds[0], &mut event) };
/// assert!(ctl_ret.is_ok());
///
/// let msg = "Hello, Rust";
/// let ret = unsafe { nc::write(fds[1], msg.as_bytes()) };
/// assert!(ret.is_ok());
///
/// let mut events = [nc::epoll_event_t::default(); 4];
/// let timeout = 0;
/// let sigmask = nc::sigset_t::default();
/// let ret = unsafe {
///     nc::epoll_pwait(
///         epfd,
///         &mut events,
///         timeout,
///         &sigmask,
///     )
/// };
/// assert!(ret.is_ok());
/// assert_eq!(ret, Ok(1));
///
/// for event in &events {
///     // Ready to read
///     if event.events == nc::EPOLLIN {
///         let ready_fd = unsafe { event.data.fd };
///         assert_eq!(ready_fd, fds[0]);
///         let mut buf = [0_u8; 64];
///         let ret = unsafe { nc::read(ready_fd, &mut buf) };
///         assert!(ret.is_ok());
///         let n_read = ret.unwrap() as usize;
///         assert_eq!(msg.as_bytes(), &buf[..n_read]);
///     }
/// }
///
/// let ret = unsafe { nc::close(fds[0]) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::close(fds[1]) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::close(epfd) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn epoll_pwait(
    epfd: i32,
    events: &mut [epoll_event_t],
    timeout: i32,
    sigmask: &sigset_t,
) -> Result<i32, Errno> {
    let epfd = epfd as usize;
    let events_ptr = events.as_mut_ptr() as usize;
    let max_events = events.len();
    let timeout = timeout as usize;
    let sigmask_ptr = sigmask as *const sigset_t as usize;
    let sigset_size = core::mem::size_of::<sigset_t>();
    syscall6(
        SYS_EPOLL_PWAIT,
        epfd,
        events_ptr,
        max_events,
        timeout,
        sigmask_ptr,
        sigset_size,
    )
    .map(|ret| ret as i32)
}

/// Wait for an I/O event on an epoll file descriptor.
///
/// # Examples
///
/// ```
/// let epfd = unsafe { nc::epoll_create1(nc::EPOLL_CLOEXEC) };
/// assert!(epfd.is_ok());
/// let epfd = epfd.unwrap();
/// let mut fds: [i32; 2] = [0, 0];
/// let ret = unsafe { nc::pipe2(&mut fds, 0) };
/// assert!(ret.is_ok());
/// let mut event = nc::epoll_event_t::default();
/// event.events = nc::EPOLLIN | nc::EPOLLET;
/// event.data.fd = fds[0];
/// let ctl_ret = unsafe { nc::epoll_ctl(epfd, nc::EPOLL_CTL_ADD, fds[0], &mut event) };
/// assert!(ctl_ret.is_ok());
///
/// let msg = "Hello, Rust";
/// let ret = unsafe { nc::write(fds[1], msg.as_bytes()) };
/// assert!(ret.is_ok());
///
/// let mut events = [nc::epoll_event_t::default(); 4];
/// let timeout = 0;
/// let sigmask = nc::sigset_t::default();
/// let ret = unsafe {
///     nc::epoll_pwait2(
///         epfd,
///         &mut events,
///         None,
///         &sigmask,
///     )
/// };
/// assert!(ret.is_ok());
/// assert_eq!(ret, Ok(1));
///
/// for event in &events {
///     // Ready to read
///     if event.events == nc::EPOLLIN {
///         let ready_fd = unsafe { event.data.fd };
///         assert_eq!(ready_fd, fds[0]);
///         let mut buf = [0_u8; 64];
///         let ret = unsafe { nc::read(ready_fd, &mut buf) };
///         assert!(ret.is_ok());
///         let n_read = ret.unwrap() as usize;
///         assert_eq!(msg.as_bytes(), &buf[..n_read]);
///     }
/// }
///
/// let ret = unsafe { nc::close(fds[0]) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::close(fds[1]) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::close(epfd) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn epoll_pwait2(
    epfd: i32,
    events: &mut [epoll_event_t],
    timeout: Option<&timespec_t>,
    sigmask: &sigset_t,
) -> Result<i32, Errno> {
    let epfd = epfd as usize;
    let events_ptr = events.as_mut_ptr() as usize;
    let max_events = events.len();
    let timeout_ptr = timeout.map_or(core::ptr::null::<timespec_t>() as usize, |timeout| {
        timeout as *const timespec_t as usize
    });
    let sigmask_ptr = sigmask as *const sigset_t as usize;
    let sigset_size = core::mem::size_of::<sigset_t>();
    syscall6(
        SYS_EPOLL_PWAIT2,
        epfd,
        events_ptr,
        max_events,
        timeout_ptr,
        sigmask_ptr,
        sigset_size,
    )
    .map(|ret| ret as i32)
}

/// Create a file descriptor for event notification.
pub unsafe fn eventfd2(count: u32, flags: i32) -> Result<i32, Errno> {
    let count = count as usize;
    let flags = flags as usize;
    syscall2(SYS_EVENTFD2, count, flags).map(|ret| ret as i32)
}

/// Execute a new program.
///
/// # Examples
///
/// ```
/// let args = ["ls", "-l", "-a"];
/// let env = ["DISPLAY=:0"];
/// let ret = unsafe { nc::execve("/bin/ls", &args, &env) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn execve<P: AsRef<Path>>(filename: P, argv: &[P], env: &[P]) -> Result<(), Errno> {
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;

    // Construct argument list.
    let argv_data: Vec<CString> = argv.iter().map(|arg| CString::new(arg.as_ref())).collect();
    let mut argv_data_ptr: Vec<*const u8> = argv_data.iter().map(|arg| arg.as_ptr()).collect();
    // Null-terminated
    argv_data_ptr.push(core::ptr::null::<u8>());
    let argv_ptr = argv_data_ptr.as_ptr() as usize;

    // Construct environment list.
    let env_data: Vec<CString> = env.iter().map(|item| CString::new(item.as_ref())).collect();
    let mut env_data_ptr: Vec<*const u8> = env_data.iter().map(|item| item.as_ptr()).collect();
    // Null-terminated
    env_data_ptr.push(core::ptr::null::<u8>());
    let env_ptr = env_data_ptr.as_ptr() as usize;

    syscall3(SYS_EXECVE, filename_ptr, argv_ptr, env_ptr).map(drop)
}

/// Execute a new program relative to a directory file descriptor.
///
/// # Examples
///
/// Specify program file via `filename`:
///
/// ```
/// let args = ["ls", "-l", "-a"];
/// let env = ["DISPLAY=:0"];
/// let ret = unsafe { nc::execveat(nc::AT_FDCWD, "/bin/ls", &args, &env, 0) };
/// assert!(ret.is_ok());
/// ```
///
/// Or via an opened file descriptor `fd`, leaving `filename` empty:
///
/// ```
/// let args = ["ls", "-l", "-a"];
/// let env = ["DISPLAY=:0"];
/// let ret = unsafe { nc::openat(nc::AT_FDCWD, "/bin/ls", nc::O_RDONLY | nc::O_CLOEXEC, 0) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
/// let ret = unsafe { nc::execveat(fd, "", &args, &env, nc::AT_EMPTY_PATH) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn execveat<P: AsRef<Path>>(
    fd: i32,
    filename: P,
    argv: &[P],
    env: &[P],
    flags: i32,
) -> Result<(), Errno> {
    let fd = fd as usize;
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;

    // Construct argument list.
    let argv_data: Vec<CString> = argv.iter().map(|arg| CString::new(arg.as_ref())).collect();
    let mut argv_data_ptr: Vec<*const u8> = argv_data.iter().map(|arg| arg.as_ptr()).collect();
    // Null-terminated
    argv_data_ptr.push(core::ptr::null::<u8>());
    let argv_ptr = argv_data_ptr.as_ptr() as usize;

    // Construct environment list.
    let env_data: Vec<CString> = env.iter().map(|item| CString::new(item.as_ref())).collect();
    let mut env_data_ptr: Vec<*const u8> = env_data.iter().map(|item| item.as_ptr()).collect();
    // Null-terminated
    env_data_ptr.push(core::ptr::null::<u8>());
    let env_ptr = env_data_ptr.as_ptr() as usize;

    let flags = flags as usize;
    syscall5(SYS_EXECVEAT, fd, filename_ptr, argv_ptr, env_ptr, flags).map(drop)
}

/// Terminate current process.
///
/// # Examples
///
/// ```
/// unsafe { nc::exit(0); }
/// ```
pub unsafe fn exit(status: i32) -> ! {
    let status = status as usize;
    let _ret = syscall1(SYS_EXIT, status);
    unreachable!();
}

/// Exit all threads in a process's thread group.
///
/// # Examples
///
/// ```
/// unsafe { nc::exit_group(0); }
/// ```
pub unsafe fn exit_group(status: i32) -> ! {
    let status = status as usize;
    let _ret = syscall1(SYS_EXIT_GROUP, status);
    unreachable!();
}

/// Check user's permission for a file.
///
/// # Examples
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

/// Check user's permission for a file.
///
/// # Examples
///
/// ```
/// let ret = unsafe { nc::faccessat2(nc::AT_FDCWD, "/etc/passwd", nc::F_OK, nc::AT_SYMLINK_NOFOLLOW) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn faccessat2<P: AsRef<Path>>(
    dfd: i32,
    filename: P,
    mode: i32,
    flags: i32,
) -> Result<(), Errno> {
    let dfd = dfd as usize;
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    let mode = mode as usize;
    let flags = flags as usize;
    syscall4(SYS_FACCESSAT2, dfd, filename_ptr, mode, flags).map(drop)
}

/// Predeclare an access pattern for file data.
///
/// # Examples
///
/// ```
/// let path = "/etc/passwd";
/// let ret = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_RDONLY, 0) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
/// let ret = unsafe { nc::fadvise64(fd, 0, 1024, nc::POSIX_FADV_NORMAL) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn fadvise64(fd: i32, offset: loff_t, len: size_t, advice: i32) -> Result<(), Errno> {
    let fd = fd as usize;
    let offset = offset as usize;
    let advice = advice as usize;
    syscall4(SYS_FADVISE64, fd, offset, len, advice).map(drop)
}

/// Manipulate file space.
///
/// # Examples
///
/// ```
/// let path = "/tmp/nc-fallocate";
/// let fd = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_WRONLY | nc::O_CREAT, 0o644) };
/// assert!(fd.is_ok());
/// let fd = fd.unwrap();
/// let ret = unsafe { nc::fallocate(fd, 0, 0, 64 * 1024) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, path, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn fallocate(fd: i32, mode: i32, offset: loff_t, len: loff_t) -> Result<(), Errno> {
    let fd = fd as usize;
    let mode = mode as usize;
    let offset = offset as usize;
    let len = len as usize;
    syscall4(SYS_FALLOCATE, fd, mode, offset, len).map(drop)
}

/// Create and initialize fanotify group.
pub unsafe fn fanotify_init(flags: u32, event_f_flags: u32) -> Result<i32, Errno> {
    let flags = flags as usize;
    let event_f_flags = event_f_flags as usize;
    syscall2(SYS_FANOTIFY_INIT, flags, event_f_flags).map(|ret| ret as i32)
}

/// Add, remove, or modify an fanotify mark on a filesystem object
pub unsafe fn fanotify_mark<P: AsRef<Path>>(
    fanotify_fd: i32,
    flags: u32,
    mask: u64,
    dir_fd: i32,
    filename: Option<P>,
) -> Result<(), Errno> {
    let fanotify_fd = fanotify_fd as usize;
    let flags = flags as usize;
    let mask = mask as usize;
    let dir_fd = dir_fd as usize;
    let filename = filename.map(|filename| CString::new(filename.as_ref()));
    let filename_ptr = filename.map_or(core::ptr::null::<u8>() as usize, |filename| {
        filename.as_ptr() as usize
    });
    syscall5(
        SYS_FANOTIFY_MARK,
        fanotify_fd,
        flags,
        mask,
        dir_fd,
        filename_ptr,
    )
    .map(drop)
}

/// Change working directory.
///
/// # Examples
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

/// Change permissions of a file.
///
/// # Examples
///
/// ```
/// let filename = "/tmp/nc-fchmod";
/// let fd = unsafe {
///     nc::openat(
///         nc::AT_FDCWD,
///         filename,
///         nc::O_CREAT | nc::O_WRONLY,
///         0o644
///     )
/// };
/// assert!(fd.is_ok());
/// let fd = fd.unwrap();
/// let ret = unsafe { nc::fchmod(fd, 0o640) };
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
/// # Examples
///
/// ```
/// let filename = "/tmp/nc-fchmodat";
/// let fd = unsafe {
///     nc::openat(
///         nc::AT_FDCWD,
///         filename,
///         nc::O_CREAT | nc::O_WRONLY,
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

/// Change permissions of a file.
///
/// Like `fchmodat()`, with specifying extra `flags`.
///
/// # Examples
///
/// ```
/// let filename = "/tmp/nc-fchmodat2";
/// let fd = unsafe {
///     nc::openat(
///         nc::AT_FDCWD,
///         filename,
///         nc::O_CREAT | nc::O_WRONLY,
///         0o644
///     )
/// };
/// assert!(fd.is_ok());
/// let fd = fd.unwrap();
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::fchmodat2(nc::AT_FDCWD, filename, 0o600, nc::AT_SYMLINK_NOFOLLOW as u32) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, filename, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn fchmodat2<P: AsRef<Path>>(
    dirfd: i32,
    filename: P,
    mode: mode_t,
    flags: u32,
) -> Result<(), Errno> {
    let dirfd = dirfd as usize;
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    let mode = mode as usize;
    let flags = flags as usize;
    syscall4(SYS_FCHMODAT, dirfd, filename_ptr, mode, flags).map(drop)
}

/// Change ownership of a file.
///
/// # Examples
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
/// # Examples
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

/// manipulate file descriptor.
///
/// # Examples
///
/// ```
/// let path = "/etc/passwd";
/// let ret = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_RDONLY, 0) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
///
/// let ret = unsafe { nc::fcntl(fd, nc::F_DUPFD, std::ptr::null()) };
/// assert!(ret.is_ok());
/// let fd2 = ret.unwrap();
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::close(fd2) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn fcntl(fd: i32, cmd: u32, arg: *const core::ffi::c_void) -> Result<i32, Errno> {
    let fd = fd as usize;
    let cmd = cmd as usize;
    let arg = arg as usize;
    syscall3(SYS_FCNTL, fd, cmd, arg).map(|ret| ret as i32)
}

/// Flush all modified in-core data (exclude metadata) refered by `fd` to disk.
///
/// # Examples
///
/// ```
/// let path = "/tmp/nc-fdatasync";
/// let fd = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_WRONLY | nc::O_CREAT, 0o644) };
/// assert!(fd.is_ok());
/// let fd = fd.unwrap();
/// let msg = b"Hello, Rust";
/// let ret = unsafe { nc::write(fd, msg) };
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

/// Get extended attribute value.
///
/// # Examples
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
///         attr_value.as_bytes(),
///         flags,
///     )
/// };
/// assert!(ret.is_ok());
/// let mut buf = [0_u8; 16];
/// let buf_len = buf.len();
/// let ret = unsafe { nc::fgetxattr(fd, attr_name, &mut buf) };
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
    value: &mut [u8],
) -> Result<ssize_t, Errno> {
    let fd = fd as usize;
    let name = CString::new(name.as_ref());
    let name_ptr = name.as_ptr() as usize;
    let value_ptr = value.as_mut_ptr() as usize;
    let size = value.len();
    syscall4(SYS_FGETXATTR, fd, name_ptr, value_ptr, size).map(|ret| ret as ssize_t)
}

/// Load a kernel module.
pub unsafe fn finit_module<P: AsRef<Path>>(
    fd: i32,
    param_values: P,
    flags: i32,
) -> Result<(), Errno> {
    let fd = fd as usize;
    let param_values = CString::new(param_values.as_ref());
    let param_values_ptr = param_values.as_ptr() as usize;
    let flags = flags as usize;
    syscall3(SYS_FINIT_MODULE, fd, param_values_ptr, flags).map(drop)
}

/// List extended attribute names.
///
/// # Examples
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
///         attr_value.as_bytes(),
///         flags,
///     )
/// };
/// assert!(ret.is_ok());
/// let mut buf = [0_u8; 16];
/// let buf_len = buf.len();
/// let ret = unsafe { nc::flistxattr(fd, &mut buf) };
/// let attr_len = ret.unwrap() as usize;
/// assert_eq!(&buf[..attr_len - 1], attr_name.as_bytes());
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, path, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn flistxattr(fd: i32, value: &mut [u8]) -> Result<ssize_t, Errno> {
    let fd = fd as usize;
    let value_ptr = value.as_mut_ptr() as usize;
    let size = value.len();
    syscall3(SYS_FLISTXATTR, fd, value_ptr, size).map(|ret| ret as ssize_t)
}

/// Apply or remove an advisory lock on an open file.
///
/// # Examples
///
/// ```
/// let path = "/tmp/nc-flock";
/// let ret = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_WRONLY | nc::O_CREAT, 0o644) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
/// let ret = unsafe { nc::flock(fd, nc::LOCK_EX) };
/// assert!(ret.is_ok());
/// let msg = "Hello, Rust";
/// let ret = unsafe { nc::write(fd, msg.as_bytes()) };
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

/// Remove an extended attribute.
///
/// # Examples
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
///         attr_value.as_bytes(),
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

/// Set parameters and trigger actions on a context.
pub unsafe fn fsconfig<P: AsRef<Path>>(
    fd: i32,
    cmd: u32,
    key: P,
    value: P,
    aux: i32,
) -> Result<(), Errno> {
    let fd = fd as usize;
    let cmd = cmd as usize;
    let key = CString::new(key.as_ref());
    let key_ptr = key.as_ptr() as usize;
    let value = CString::new(value.as_ref());
    let value_ptr = value.as_ptr() as usize;
    let aux = aux as usize;
    syscall5(SYS_FSCONFIG, fd, cmd, key_ptr, value_ptr, aux).map(drop)
}

/// Set extended attribute value.
///
/// # Examples
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
///         attr_value.as_bytes(),
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
    value: &[u8],
    flags: i32,
) -> Result<(), Errno> {
    let fd = fd as usize;
    let name = CString::new(name.as_ref());
    let name_ptr = name.as_ptr() as usize;
    let value_ptr = value.as_ptr() as usize;
    let size = value.len();
    let flags = flags as usize;
    syscall5(SYS_FSETXATTR, fd, name_ptr, value_ptr, size, flags).map(drop)
}

/// Create a kernel mount representation for a new, prepared superblock.
pub unsafe fn fsmount(fs_fd: i32, flags: u32, attr_flags: u32) -> Result<i32, Errno> {
    let fs_fd = fs_fd as usize;
    let flags = flags as usize;
    let attr_flags = attr_flags as usize;
    syscall3(SYS_FSMOUNT, fs_fd, flags, attr_flags).map(|ret| ret as i32)
}

/// Open a filesystem by name so that it can be configured for mounting.
pub unsafe fn fsopen<P: AsRef<Path>>(fs_name: P, flags: u32) -> Result<(), Errno> {
    let fs_name = CString::new(fs_name.as_ref());
    let fs_name_ptr = fs_name.as_ptr() as usize;
    let flags = flags as usize;
    syscall2(SYS_FSOPEN, fs_name_ptr, flags).map(drop)
}

/// Pick a superblock into a context for reconfiguration.
pub unsafe fn fspick<P: AsRef<Path>>(dfd: i32, path: P, flags: u32) -> Result<i32, Errno> {
    let dfd = dfd as usize;
    let path = CString::new(path.as_ref());
    let path_ptr = path.as_ptr() as usize;
    let flags = flags as usize;
    syscall3(SYS_FSPICK, dfd, path_ptr, flags).map(|ret| ret as i32)
}

/// Get file status about a file descriptor.
///
/// # example
///
/// ```
/// let path = "/tmp";
/// // Open folder directly.
/// let fd = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_PATH, 0) };
/// assert!(fd.is_ok());
/// let fd = fd.unwrap();
/// let mut stat = nc::stat_t::default();
/// let ret = unsafe { nc::fstat(fd, &mut stat) };
/// assert!(ret.is_ok());
/// // Check fd is a directory.
/// assert!(nc::S_ISDIR(stat.st_mode as nc::mode_t));
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn fstat(fd: i32, statbuf: &mut stat_t) -> Result<(), Errno> {
    let fd = fd as usize;
    let statbuf_ptr = statbuf as *mut stat_t as usize;
    syscall2(SYS_FSTAT, fd, statbuf_ptr).map(drop)
}

/// Get filesystem statistics.
///
/// # Examples
///
/// ```
/// let path = "/usr";
/// // Open folder directly.
/// let fd = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_PATH, 0) };
/// assert!(fd.is_ok());
/// let fd = fd.unwrap();
/// let mut statfs = nc::statfs_t::default();
/// let ret = unsafe { nc::fstatfs(fd, &mut statfs) };
/// assert!(ret.is_ok());
/// assert!(statfs.f_bfree > 0);
/// assert!(statfs.f_bavail > 0);
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn fstatfs(fd: i32, buf: &mut statfs_t) -> Result<(), Errno> {
    let fd = fd as usize;
    let buf_ptr = buf as *mut statfs_t as usize;
    syscall2(SYS_FSTATFS, fd, buf_ptr).map(drop)
}

/// Flush all modified in-core data refered by `fd` to disk.
///
/// # Examples
///
/// ```
/// let path = "/tmp/nc-fsync";
/// let fd = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_CREAT | nc::O_WRONLY, 0o644) };
/// assert!(fd.is_ok());
/// let fd = fd.unwrap();
/// let buf = b"Hello, Rust";
/// let n_write = unsafe { nc::write(fd, buf) };
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

/// Truncate an opened file to a specified length.
///
/// # Examples
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

/// Fast user-space locking.
///
/// Parameters
/// - `uaddr`: futex user address
/// - `op`: futex operations
/// - `val`: expected value
/// - `utime`: waiting timeout
/// - `uaddr2`: target futext user address used for requeue
///
/// # Exampless
///
/// ```rust
/// use std::sync::atomic::{AtomicU32, Ordering};
/// use std::thread;
/// use std::time::Duration;
///
/// const NOTIFY_WAIT: u32 = 0;
/// const NOTIFY_WAKE: u32 = 1;
///
/// fn wake_one(count: &AtomicU32) {
///     let ret = unsafe { nc::futex(count, nc::FUTEX_WAKE, NOTIFY_WAKE, None, None, 0) };
///     assert!(ret.is_ok());
/// }
///
/// fn wait(count: &AtomicU32, expected: u32) {
///     let ret = unsafe { nc::futex(count, nc::FUTEX_WAIT, expected, None, None, 0) };
///     assert!(ret.is_ok());
/// }
///
/// fn main() {
///     let notify = AtomicU32::new(0);
///
///     thread::scope(|s| {
///         // Create the notify thread.
///         s.spawn(|| {
///             // Wake up some other threads after one second.
///             println!("[notify] Sleep for 1s");
///             thread::sleep(Duration::from_secs(1));
///             println!("[notify] Wake up main thread");
///             notify.store(NOTIFY_WAKE, Ordering::Relaxed);
///             wake_one(&notify);
///         });
///
///         // Main thread will wait until the notify thread wakes it up.
///         println!("[main] Waiting for notification..");
///         while notify.load(Ordering::Relaxed) == NOTIFY_WAIT {
///             wait(&notify, NOTIFY_WAIT);
///         }
///         println!("[main] Got wake up");
///     });
/// }
/// ```
pub unsafe fn futex(
    uaddr: &core::sync::atomic::AtomicU32,
    op: i32,
    val: u32,
    utime: Option<&timespec_t>,
    uaddr2: Option<&core::sync::atomic::AtomicU32>,
    val3: u32,
) -> Result<i32, Errno> {
    use core::sync::atomic::AtomicU32;
    let uaddr_ptr = uaddr as *const AtomicU32 as usize;
    let op = op as usize;
    let val = val as usize;
    let utime_ptr = utime.map_or(core::ptr::null::<timespec_t>() as usize, |time_ref| {
        time_ref as *const timespec_t as usize
    });
    let uaddr2_ptr = uaddr2.map_or(core::ptr::null::<AtomicU32>() as usize, |uaddr2_ref| {
        uaddr2_ref as *const AtomicU32 as usize
    });
    let val3 = val3 as usize;
    syscall6(SYS_FUTEX, uaddr_ptr, op, val, utime_ptr, uaddr2_ptr, val3).map(|ret| ret as i32)
}

/// Requeue a waiter from one futex to another.
///
/// - `waiters`: array describing the source and destination futex
/// - `flags`: unused
/// - `nr_wake`: number of futexes to wake
/// - `nr_requeue`: number of futexes to requeue
///
/// Identical to the traditional `FUTEX_CMP_REQUEUE` op, except it is part of the
/// futex2 family of calls.
pub unsafe fn futex_requeue(
    waiters: &mut [futex_waitv_t],
    flags: u32,
    nr_wake: i32,
    nr_requeue: i32,
) -> Result<(), Errno> {
    let waiters_ptr = waiters.as_mut_ptr() as usize;
    let flags = flags as usize;
    let nr_wake = nr_wake as usize;
    let nr_requeue = nr_requeue as usize;
    syscall4(SYS_FUTEX_REQUEUE, waiters_ptr, flags, nr_wake, nr_requeue).map(drop)
}

/// Wait on a futex.
///
/// - `uaddr`: Address of the futex to wait on
/// - `val`: Value of `uaddr`
/// - `mask`: bitmask
/// - `flags`: `FUTEX2` flags
/// - `timeout`: Optional absolute timeout
/// - `clockid`: Clock to be used for the timeout, realtime or monotonic
///
/// Identical to the traditional `FUTEX_WAIT_BITSET` op, except it is part of the
/// futex2 familiy of calls.
pub unsafe fn futex_wait(
    uaddr: *const core::ffi::c_void,
    val: usize,
    mask: usize,
    flags: u32,
    timeout: Option<&timespec_t>,
    clockid: clockid_t,
) -> Result<(), Errno> {
    let uaddr = uaddr as usize;
    let flags = flags as usize;
    let timeout_ptr = timeout.map_or(core::ptr::null::<timespec_t>() as usize, |timeout| {
        timeout as *const timespec_t as usize
    });
    let clockid = clockid as usize;
    syscall6(
        SYS_FUTEX_WAIT,
        uaddr,
        val,
        mask,
        flags,
        timeout_ptr,
        clockid,
    )
    .map(drop)
}

/// Wait on a list of futexes.
///
/// - `waiters`: List of futexes to wait on
/// - `nr_futexes`: Length of futexv
/// - `flags`: Flag for timeout (monotonic/realtime)
/// - `timeout`: Optional absolute timeout.
/// - `clockid`: Clock to be used for the timeout, realtime or monotonic.
///
/// Given an array of `struct futex_waitv_t`, wait on each uaddr.
/// The thread wakes if a `futex_wake()` is performed at any uaddr.
/// The syscall returns immediately if any waiter has `*uaddr != val`.
///
/// `timeout` is an optional timeout value for the operation.
///
/// Each waiter has individual flags. The `flags` argument for the syscall
/// should be used solely for specifying the timeout as realtime, if needed.
/// Flags for private futexes, sizes, etc. should be used on the individual flags
/// of each waiter.
///
/// Returns the array index of one of the woken futexes. No further information
/// is provided: any number of other futexes may also have been woken by the
/// same event, and if more than one futex was woken, the retrned index may
/// refer to any one of them. (It is not necessaryily the futex with the
/// smallest index, nor the one most recently woken, nor...)
pub unsafe fn futex_waitv(
    waiters: &[futex_waitv_t],
    flags: u32,
    timeout: Option<&timespec_t>,
    clockid: clockid_t,
) -> Result<i32, Errno> {
    let waiters_ptr = waiters.as_ptr() as usize;
    let waiters_len = waiters.len();
    let flags = flags as usize;
    let timeout_ptr = timeout.map_or(core::ptr::null::<timespec_t>() as usize, |timeout| {
        timeout as *const timespec_t as usize
    });
    let clockid = clockid as usize;
    syscall5(
        SYS_FUTEX_WAITV,
        waiters_ptr,
        waiters_len,
        flags,
        timeout_ptr,
        clockid,
    )
    .map(|ret| ret as i32)
}

/// Wake a number of futexes.
///
/// - `uaddr`: Address of the futex(es) to wake
/// - `mask`: bitmask
/// - `nr`: Number of the futexes to wake
/// - `flags`: `FUTEX2` flags
///
/// Identical to the traditional `FUTEX_WAKE_BITSET` op, except it is part of the
/// futex2 family of calls.
pub unsafe fn futex_wake(
    uaddr: *const core::ffi::c_void,
    mask: usize,
    nr: i32,
    flags: u32,
) -> Result<(), Errno> {
    let uaddr = uaddr as usize;
    let nr = nr as usize;
    let flags = flags as usize;
    syscall4(SYS_FUTEX_WAKE, uaddr, mask, nr, flags).map(drop)
}

/// Determine CPU and NUMA node on which the calling thread is running.
///
/// # Examples
///
/// ```
/// let mut cpu = 0;
/// let mut node = 0;
/// let mut cache = nc::getcpu_cache_t::default();
/// let ret = unsafe { nc::getcpu(&mut cpu, &mut node, &mut cache) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn getcpu(
    cpu: &mut u32,
    node: &mut u32,
    cache: &mut getcpu_cache_t,
) -> Result<(), Errno> {
    let cpu_ptr = cpu as *mut u32 as usize;
    let node_ptr = node as *mut u32 as usize;
    let cache_ptr = cache as *mut getcpu_cache_t as usize;
    syscall3(SYS_GETCPU, cpu_ptr, node_ptr, cache_ptr).map(drop)
}

/// Get current working directory.
///
/// # Examples
///
/// ```
/// let mut buf = [0_u8; nc::PATH_MAX as usize + 1];
/// let ret = unsafe { nc::getcwd(&mut buf) };
/// assert!(ret.is_ok());
/// // Remove null-terminal char.
/// let path_len = ret.unwrap() as usize - 1;
/// let cwd = std::str::from_utf8(&buf[..path_len]);
/// assert!(cwd.is_ok());
/// ```
pub unsafe fn getcwd(buf: &mut [u8]) -> Result<ssize_t, Errno> {
    let buf_ptr = buf.as_mut_ptr() as usize;
    let size = buf.len();
    syscall2(SYS_GETCWD, buf_ptr, size).map(|ret| ret as ssize_t)
}

/// Get directory entries.
///
/// # Examples
///
/// ```rust
/// const BUF_SIZE: usize = 1 * 1024;
///
/// let path = "/etc";
/// let ret = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_DIRECTORY, 0) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
/// let mut buf = [0; BUF_SIZE];
///
/// loop {
///     let ret = unsafe { nc::getdents64(fd, &mut buf) };
///     assert!(ret.is_ok());
///     let nread = ret.unwrap() as usize;
///     if nread == 0 {
///         break;
///     }
///
///     let buf_ptr = buf.as_ptr() as usize;
///     let mut bpos: usize = 0;
///
///     println!("--------------- nread={nread} ---------------");
///     println!("inode#    file type  d_reclen  d_off   d_name");
///     while bpos < nread {
///         let d = (buf_ptr + bpos) as *mut nc::linux_dirent64_t;
///         let d_ref: &nc::linux_dirent64_t = unsafe { &(*d) };
///         let d_type = match d_ref.d_type {
///             nc::DT_REG => "regular",
///             nc::DT_DIR => "directory",
///             nc::DT_FIFO => "FIFO",
///             nc::DT_SOCK => "socket",
///             nc::DT_LNK => "symlink",
///             nc::DT_BLK => "block-dev",
///             nc::DT_CHR => "char-dev",
///             _ => "unknown",
///         };
///
///         let name = std::str::from_utf8(d_ref.name()).unwrap();
///         println!(
///             "{: >8}  {:<10} {: >4} {: >12}  {}",
///             d_ref.d_ino, d_type, d_ref.d_reclen, d_ref.d_off as u32, name
///         );
///
///         bpos += d_ref.d_reclen as usize;
///     }
/// }
///
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn getdents64(fd: i32, dir_buf: &mut [u8]) -> Result<ssize_t, Errno> {
    let fd = fd as usize;
    let count = dir_buf.len();
    let dir_buf_ptr = dir_buf.as_mut_ptr() as usize;
    syscall3(SYS_GETDENTS64, fd, dir_buf_ptr, count).map(|ret| ret as ssize_t)
}

/// Get the effective group ID of the calling process.
///
/// # Examples
///
/// ```
/// let egid = unsafe { nc::getegid() };
/// assert!(egid > 0);
/// ```
#[must_use]
pub unsafe fn getegid() -> gid_t {
    // This function is always successful.
    syscall0(SYS_GETEGID).unwrap_or_default() as gid_t
}

/// Get the effective user ID of the calling process.
///
/// # Examples
///
/// ```
/// let euid = unsafe { nc::geteuid() };
/// assert!(euid > 0);
/// ```
#[must_use]
pub unsafe fn geteuid() -> uid_t {
    // This function is always successful.
    syscall0(SYS_GETEUID).unwrap_or_default() as uid_t
}

/// Get the real group ID of the calling process.
///
/// # Examples
///
/// ```
/// let gid = unsafe { nc::getgid() };
/// assert!(gid > 0);
/// ```
#[must_use]
pub unsafe fn getgid() -> gid_t {
    // This function is always successful.
    syscall0(SYS_GETGID).unwrap_or_default() as gid_t
}

/// Get list of supplementary group Ids.
///
/// # Examples
///
/// ```
/// let mut groups = vec![];
/// let ret = unsafe { nc::getgroups(&mut groups) };
/// assert!(ret.is_ok());
/// let total_num = ret.unwrap();
/// groups.resize(total_num as usize, 0);
///
/// let ret = unsafe { nc::getgroups(&mut groups) };
/// assert!(ret.is_ok());
/// assert_eq!(ret, Ok(total_num));
/// ```
pub unsafe fn getgroups(group_list: &mut [gid_t]) -> Result<i32, Errno> {
    let size = group_list.len();
    let group_ptr = group_list.as_mut_ptr() as usize;
    syscall2(SYS_GETGROUPS, size, group_ptr).map(|ret| ret as i32)
}

/// Get value of an interval timer.
///
/// # Examples
///
/// ```
/// fn handle_alarm(signum: i32) {
///     assert_eq!(signum, nc::SIGALRM);
///     let msg = b"Hello alarm\n";
///     let stderr = 2;
///     let _ = unsafe { nc::write(stderr, msg) };
/// }
///
/// let sa = nc::new_sigaction(handle_alarm);
/// let ret = unsafe { nc::rt_sigaction(nc::SIGALRM, Some(&sa), None) };
/// assert!(ret.is_ok());
///
/// // Single shot timer, actived after 1 second.
/// let itv = nc::itimerval_t {
///     it_value: nc::timeval_t {
///         tv_sec: 1,
///         tv_usec: 0,
///     },
///     it_interval: nc::timeval_t {
///         tv_sec: 0,
///         tv_usec: 0,
///     },
/// };
/// let mut prev_itv = nc::itimerval_t::default();
/// let ret = unsafe { nc::setitimer(nc::ITIMER_REAL, &itv, Some(&mut prev_itv)) };
/// assert!(ret.is_ok());
///
/// let ret = unsafe { nc::getitimer(nc::ITIMER_REAL, &mut prev_itv) };
/// assert!(ret.is_ok());
/// assert!(prev_itv.it_value.tv_sec <= itv.it_value.tv_sec);
///
/// let mask = nc::sigset_t::default();
/// let _ret = unsafe { nc::rt_sigsuspend(&mask) };
///
/// let ret = unsafe { nc::getitimer(nc::ITIMER_REAL, &mut prev_itv) };
/// assert!(ret.is_ok());
/// assert_eq!(prev_itv.it_value.tv_sec, 0);
/// assert_eq!(prev_itv.it_value.tv_usec, 0);
/// ```
pub unsafe fn getitimer(which: i32, curr_val: &mut itimerval_t) -> Result<(), Errno> {
    let which = which as usize;
    let curr_val_ptr = curr_val as *mut itimerval_t as usize;
    syscall2(SYS_GETITIMER, which, curr_val_ptr).map(drop)
}

/// Get name of connected peer socket.
///
/// # Examples
///
/// ```
/// use nc::Errno;
/// use std::mem::{size_of, transmute};
/// use std::thread;
///
/// const SERVER_PORT: u16 = 18084;
///
/// #[must_use]
/// #[inline]
/// const fn htons(host: u16) -> u16 {
///     host.to_be()
/// }
///
/// fn main() -> Result<(), Errno> {
///     let listen_fd = unsafe { nc::socket(nc::AF_INET, nc::SOCK_STREAM, 0)? };
///     println!("listen fd: {listen_fd}");
///
///     let addr = nc::sockaddr_in_t {
///         sin_family: nc::AF_INET as nc::sa_family_t,
///         sin_port: htons(SERVER_PORT),
///         sin_addr: nc::in_addr_t {
///             s_addr: nc::INADDR_ANY as u32,
///         },
///         ..Default::default()
///     };
///     println!("addr: {addr:?}");
///
///     let ret = unsafe {
///         let addr_alias = transmute::<&nc::sockaddr_in_t, &nc::sockaddr_t>(&addr);
///         nc::bind(listen_fd, addr_alias, size_of::<nc::sockaddr_in_t>() as u32)
///     };
///     assert!(ret.is_ok());
///
///     // Start worker thread
///     thread::spawn(|| {
///         println!("worker thread started");
///         let socket_fd = unsafe { nc::socket(nc::AF_INET, nc::SOCK_STREAM, 0) };
///         assert!(socket_fd.is_ok());
///         if let Ok(socket_fd) = socket_fd {
///             let addr = nc::sockaddr_in_t {
///                 sin_family: nc::AF_INET as nc::sa_family_t,
///                 sin_port: htons(SERVER_PORT),
///                 sin_addr: nc::in_addr_t {
///                     s_addr: nc::INADDR_ANY as u32,
///                 },
///                 ..Default::default()
///             };
///             unsafe {
///                 let addr_alias = transmute::<&nc::sockaddr_in_t, &nc::sockaddr_t>(&addr);
///                 let ret = nc::connect(socket_fd, addr_alias, size_of::<nc::sockaddr_in_t>() as u32);
///                 assert_eq!(ret, Ok(()));
///             }
///         } else {
///             eprintln!("Failed to create socket");
///         }
///     });
///
///     unsafe {
///         nc::listen(listen_fd, nc::SOCK_STREAM)?;
///     }
///
///     let conn_fd = unsafe {
///         nc::accept4(listen_fd, None, None, nc::SOCK_CLOEXEC)?
///     };
///     println!("conn_fd: {conn_fd}");
///
///     let mut conn_addr = nc::sockaddr_in_t::default();
///     let mut conn_addr_len: nc::socklen_t = 0;
///     unsafe {
///         let _ = nc::getpeername(conn_fd,
///             &mut conn_addr as *mut nc::sockaddr_in_t as *mut nc::sockaddr_t,
///             &mut conn_addr_len
///         );
///     }
///
///     unsafe {
///         nc::close(listen_fd)?;
///     }
///
///     Ok(())
/// }
pub unsafe fn getpeername(
    sockfd: i32,
    addr: *mut sockaddr_t,
    addrlen: &mut socklen_t,
) -> Result<(), Errno> {
    let sockfd = sockfd as usize;
    let addr_ptr = addr as usize;
    let addrlen_ptr = addrlen as *mut socklen_t as usize;
    syscall3(SYS_GETPEERNAME, sockfd, addr_ptr, addrlen_ptr).map(drop)
}

/// Returns the PGID(process group ID) of the process specified by `pid`.
///
/// # Examples
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

/// Get the process ID (PID) of the calling process.
///
/// # Examples
///
/// ```
/// let pid = unsafe { nc::getpid() };
/// assert!(pid > 0);
/// ```
#[must_use]
pub unsafe fn getpid() -> pid_t {
    // This function is always successful.
    syscall0(SYS_GETPID).unwrap_or_default() as pid_t
}

/// Get the process ID of the parent of the calling process.
///
/// # Examples
///
/// ```
/// let ppid = unsafe { nc::getppid() };
/// assert!(ppid > 0);
/// ```
#[must_use]
pub unsafe fn getppid() -> pid_t {
    // This function is always successful.
    syscall0(SYS_GETPPID).unwrap_or_default() as pid_t
}

/// Get program scheduling priority.
///
/// # Examples
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

/// Obtain a series of random bytes.
///
/// # Examples
///
/// ```
/// let mut buf = [0_u8; 32];
/// let ret = unsafe { nc::getrandom(&mut buf, 0) };
/// assert!(ret.is_ok());
/// let size = ret.unwrap() as usize;
/// assert!(size <= buf.len());
/// ```
pub unsafe fn getrandom(buf: &mut [u8], flags: u32) -> Result<ssize_t, Errno> {
    let buf_ptr = buf.as_mut_ptr() as usize;
    let buf_len = buf.len();
    let flags = flags as usize;
    syscall3(SYS_GETRANDOM, buf_ptr, buf_len, flags).map(|ret| ret as ssize_t)
}

/// Get real, effect and saved group ID.
///
/// # Examples
///
/// ```
/// let mut rgid = 0;
/// let mut egid = 0;
/// let mut sgid = 0;
/// let ret = unsafe { nc::getresgid(&mut rgid, &mut egid, &mut sgid) };
/// assert!(ret.is_ok());
/// assert!(rgid > 0);
/// assert!(egid > 0);
/// assert!(sgid > 0);
/// ```
pub unsafe fn getresgid(rgid: &mut gid_t, egid: &mut gid_t, sgid: &mut gid_t) -> Result<(), Errno> {
    let rgid_ptr = rgid as *mut gid_t as usize;
    let egid_ptr = egid as *mut gid_t as usize;
    let sgid_ptr = sgid as *mut gid_t as usize;
    syscall3(SYS_GETRESGID, rgid_ptr, egid_ptr, sgid_ptr).map(drop)
}

/// Get real, effect and saved user ID.
///
/// # Examples
///
/// ```
/// let mut ruid = 0;
/// let mut euid = 0;
/// let mut suid = 0;
/// let ret = unsafe { nc::getresuid(&mut ruid, &mut euid, &mut suid) };
/// assert!(ret.is_ok());
/// assert!(ruid > 0);
/// assert!(euid > 0);
/// assert!(suid > 0);
/// ```
pub unsafe fn getresuid(ruid: &mut uid_t, euid: &mut uid_t, suid: &mut uid_t) -> Result<(), Errno> {
    let ruid_ptr = ruid as *mut uid_t as usize;
    let euid_ptr = euid as *mut uid_t as usize;
    let suid_ptr = suid as *mut uid_t as usize;
    syscall3(SYS_GETRESUID, ruid_ptr, euid_ptr, suid_ptr).map(drop)
}

/// Get resource limit.
///
/// # Examples
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

/// Get resource usage.
///
/// # Examples
///
/// ```
/// let mut usage = nc::rusage_t::default();
/// let ret = unsafe { nc::getrusage(nc::RUSAGE_SELF, &mut usage) };
/// assert!(ret.is_ok());
/// assert!(usage.ru_maxrss > 0);
/// assert_eq!(usage.ru_nswap, 0);
/// ```
pub unsafe fn getrusage(who: i32, usage: &mut rusage_t) -> Result<(), Errno> {
    let who = who as usize;
    let usage_ptr = usage as *mut rusage_t as usize;
    syscall2(SYS_GETRUSAGE, who, usage_ptr).map(drop)
}

/// Get session Id.
///
/// # Examples
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
    syscall1(SYS_GETSID, pid).unwrap_or_default() as pid_t
}

/// Get current address to which the socket `sockfd` is bound.
///
/// # Examples
///
/// ```
/// use nc::Errno;
/// use std::mem::{size_of, transmute};
/// use std::thread;
///
/// const SERVER_PORT: u16 = 18088;
///
/// #[must_use]
/// #[inline]
/// const fn htons(host: u16) -> u16 {
///     host.to_be()
/// }
///
/// fn main() -> Result<(), Errno> {
///     let listen_fd = unsafe { nc::socket(nc::AF_INET, nc::SOCK_STREAM, 0)? };
///     println!("listen fd: {listen_fd}");
///
///     let addr = nc::sockaddr_in_t {
///         sin_family: nc::AF_INET as nc::sa_family_t,
///         sin_port: htons(SERVER_PORT),
///         sin_addr: nc::in_addr_t {
///             s_addr: nc::INADDR_ANY as u32,
///         },
///         ..Default::default()
///     };
///     println!("addr: {addr:?}");
///
///     let ret = unsafe {
///         let addr_alias = transmute::<&nc::sockaddr_in_t, &nc::sockaddr_t>(&addr);
///         nc::bind(listen_fd, addr_alias, size_of::<nc::sockaddr_in_t>() as u32)
///     };
///     assert!(ret.is_ok());
///
///     // Start worker thread
///     thread::spawn(|| {
///         println!("worker thread started");
///         let socket_fd = unsafe { nc::socket(nc::AF_INET, nc::SOCK_STREAM, 0) };
///         assert!(socket_fd.is_ok());
///         if let Ok(socket_fd) = socket_fd {
///             let addr = nc::sockaddr_in_t {
///                 sin_family: nc::AF_INET as nc::sa_family_t,
///                 sin_port: htons(SERVER_PORT),
///                 sin_addr: nc::in_addr_t {
///                     s_addr: nc::INADDR_ANY as u32,
///                 },
///                 ..Default::default()
///             };
///             unsafe {
///                 let addr_alias = transmute::<&nc::sockaddr_in_t, &nc::sockaddr_t>(&addr);
///                 let ret = nc::connect(socket_fd, addr_alias, size_of::<nc::sockaddr_in_t>() as u32);
///                 assert_eq!(ret, Ok(()));
///             }
///         } else {
///             eprintln!("Failed to create socket");
///         }
///     });
///
///     unsafe {
///         nc::listen(listen_fd, nc::SOCK_STREAM)?;
///     }
///
///     let conn_fd = unsafe {
///         nc::accept4(listen_fd, None, None, nc::SOCK_CLOEXEC)?
///     };
///     println!("conn_fd: {conn_fd}");
///
///     let mut conn_addr = nc::sockaddr_in_t::default();
///     let mut conn_addr_len: nc::socklen_t = 0;
///     unsafe {
///         let _ = nc::getsockname(conn_fd,
///             &mut conn_addr as *mut nc::sockaddr_in_t as *mut nc::sockaddr_t,
///             &mut conn_addr_len
///         );
///     }
///
///     unsafe {
///         nc::close(listen_fd)?;
///     }
///
///     Ok(())
/// }
pub unsafe fn getsockname(
    sockfd: i32,
    addr: *mut sockaddr_t,
    addrlen: &mut socklen_t,
) -> Result<(), Errno> {
    let sockfd = sockfd as usize;
    let addr_ptr = addr as usize;
    let addrlen_ptr = addrlen as *mut socklen_t as usize;
    syscall3(SYS_GETSOCKNAME, sockfd, addr_ptr, addrlen_ptr).map(drop)
}

/// Get options on sockets
///
/// # Examples
/// ```
/// use std::mem::size_of_val;
///
/// fn main() -> Result<(), nc::Errno> {
///     let socket_fd = unsafe { nc::socket(nc::AF_INET, nc::SOCK_STREAM, 0)? };
///
///     let set_queue_len: i32 = 5;
///     {
///         let buf_len = size_of_val(&set_queue_len) as nc::socklen_t;
///         let ret = unsafe {
///             nc::setsockopt(
///                 socket_fd,
///                 nc::IPPROTO_TCP,
///                 nc::TCP_FASTOPEN,
///                 &set_queue_len as *const i32 as *const _,
///                 buf_len,
///             )
///         };
///         assert!(ret.is_ok());
///     }
///
///     let mut get_queue_len: i32 = 0;
///     {
///         let mut buf_len = size_of_val(&get_queue_len) as nc::socklen_t;
///         let ret = unsafe {
///             nc::getsockopt(
///                 socket_fd,
///                 nc::IPPROTO_TCP,
///                 nc::TCP_FASTOPEN,
///                 &mut get_queue_len as *mut i32 as *mut _,
///                 &mut buf_len,
///             )
///         };
///         assert!(ret.is_ok());
///         println!("queue len: {get_queue_len}");
///     }
///     assert_eq!(set_queue_len, get_queue_len);
///
///     unsafe { nc::close(socket_fd) }
/// }
/// ```
pub unsafe fn getsockopt(
    sockfd: i32,
    level: i32,
    opt_name: i32,
    opt_val: *mut core::ffi::c_void,
    opt_len: &mut socklen_t,
) -> Result<(), Errno> {
    let sockfd = sockfd as usize;
    let level = level as usize;
    let opt_name = opt_name as usize;
    let opt_val_ptr = opt_val as usize;
    let opt_len_ptr = opt_len as *mut socklen_t as usize;
    syscall5(
        SYS_GETSOCKOPT,
        sockfd,
        level,
        opt_name,
        opt_val_ptr,
        opt_len_ptr,
    )
    .map(drop)
}

/// Get the caller's thread ID (TID).
///
/// # Examples
///
/// ```
/// let tid = unsafe { nc::gettid() };
/// assert!(tid > 0);
/// ```
#[must_use]
pub unsafe fn gettid() -> pid_t {
    // This function is always successful.
    syscall0(SYS_GETTID).unwrap_or_default() as pid_t
}

/// Get time.
///
/// # Examples
///
/// ```
/// let mut tv = nc::timeval_t::default();
/// let mut tz = nc::timezone_t::default();
/// let ret = unsafe { nc::gettimeofday(&mut tv, Some(&mut tz)) };
/// assert!(ret.is_ok());
/// assert!(tv.tv_sec > 1611380386);
/// ```
pub unsafe fn gettimeofday(
    timeval: &mut timeval_t,
    tz: Option<&mut timezone_t>,
) -> Result<(), Errno> {
    let timeval_ptr = timeval as *mut timeval_t as usize;
    let tz_ptr = tz.map_or(core::ptr::null_mut::<timezone_t>() as usize, |tz| {
        tz as *mut timezone_t as usize
    });
    syscall2(SYS_GETTIMEOFDAY, timeval_ptr, tz_ptr).map(drop)
}

/// Get the real user ID of the calling process.
///
/// # Examples
///
/// ```
/// let uid = unsafe { nc::getuid() };
/// assert!(uid > 0);
/// ```
#[must_use]
pub unsafe fn getuid() -> uid_t {
    // This function is always successful.
    syscall0(SYS_GETUID).unwrap_or_default() as uid_t
}

/// Get extended attribute value.
///
/// # Examples
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
///         attr_value.as_bytes(),
///         flags,
///     )
/// };
/// assert!(ret.is_ok());
/// let mut buf = [0_u8; 16];
/// let buf_len = buf.len();
/// let ret = unsafe { nc::getxattr(path, attr_name, &mut buf) };
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
    value: &mut [u8],
) -> Result<ssize_t, Errno> {
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    let name = CString::new(name.as_ref());
    let name_ptr = name.as_ptr() as usize;
    let value_ptr = value.as_mut_ptr() as usize;
    let size = value.len();
    syscall4(SYS_GETXATTR, filename_ptr, name_ptr, value_ptr, size).map(|ret| ret as ssize_t)
}

/// Retrieve NUMA memory policy for a thread
pub unsafe fn get_mempolicy(
    mode: &mut i32,
    nmask: &mut usize,
    max_node: usize,
    addr: *const core::ffi::c_void,
    flags: usize,
) -> Result<(), Errno> {
    let mode_ptr = mode as *mut i32 as usize;
    let nmask_ptr = nmask as *mut usize as usize;
    let addr = addr as usize;
    syscall5(
        SYS_GET_MEMPOLICY,
        mode_ptr,
        nmask_ptr,
        max_node,
        addr,
        flags,
    )
    .map(drop)
}

/// Get the robust-futex list head of a task.
///
/// Params:
/// - `pid`: pid of the process `[zero for current task]`
/// - `head_ptr`: pointer to a list-head pointer, the kernel fills it in
/// - `len_ptr`: pointer to a length field, the kernel fills in the header size
pub unsafe fn get_robust_list(
    pid: pid_t,
    head_ptr: *mut *mut robust_list_head_t,
    len_ptr: &mut size_t,
) -> Result<(), Errno> {
    let pid = pid as usize;
    let head_ptr = head_ptr as usize;
    let len_ptr = len_ptr as *mut size_t as usize;
    syscall3(SYS_GET_ROBUST_LIST, pid, head_ptr, len_ptr).map(drop)
}

/// Load a kernel module.
pub unsafe fn init_module<P: AsRef<Path>>(
    module_image: &[u8],
    param_values: P,
) -> Result<(), Errno> {
    let module_image_ptr = module_image.as_ptr() as usize;
    let len = module_image.len();
    let param_values = CString::new(param_values.as_ref());
    let param_values_ptr = param_values.as_ptr() as usize;
    syscall3(SYS_INIT_MODULE, module_image_ptr, len, param_values_ptr).map(drop)
}

/// Add a watch to an initialized inotify instance.
///
/// # Examples
///
/// ```
/// let ret = unsafe { nc::inotify_init1(nc::IN_NONBLOCK | nc::IN_CLOEXEC) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
///
/// let path = "/etc/passwd";
/// let ret = unsafe { nc::inotify_add_watch(fd, path, nc::IN_MODIFY) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn inotify_add_watch<P: AsRef<Path>>(
    fd: i32,
    filename: P,
    mask: u32,
) -> Result<i32, Errno> {
    let fd = fd as usize;
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    let mask = mask as usize;
    syscall3(SYS_INOTIFY_ADD_WATCH, fd, filename_ptr, mask).map(|ret| ret as i32)
}

/// Initialize an inotify instance.
///
/// # Examples
///
/// ```
/// let ret = unsafe { nc::inotify_init1(nc::IN_NONBLOCK | nc::IN_CLOEXEC) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn inotify_init1(flags: i32) -> Result<i32, Errno> {
    let flags = flags as usize;
    syscall1(SYS_INOTIFY_INIT1, flags).map(|ret| ret as i32)
}

/// Remove an existing watch from an inotify instance.
///
/// # Examples
///
/// ```
/// let ret = unsafe { nc::inotify_init1(nc::IN_NONBLOCK | nc::IN_CLOEXEC) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
///
/// let path = "/etc/passwd";
/// let ret = unsafe { nc::inotify_add_watch(fd, path, nc::IN_MODIFY) };
/// assert!(ret.is_ok());
/// let wd = ret.unwrap();
/// let ret = unsafe { nc::inotify_rm_watch(fd, wd) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn inotify_rm_watch(fd: i32, wd: i32) -> Result<(), Errno> {
    let fd = fd as usize;
    let wd = wd as usize;
    syscall2(SYS_INOTIFY_RM_WATCH, fd, wd).map(drop)
}

/// Control device.
///
/// # Examples
///
/// ```
/// let path = "/tmp/nc-ioctl";
/// let ret = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_WRONLY | nc::O_CREAT, 0o644) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
/// let mut attr: i32 = 0;
/// let cmd = nc::FS_IOC_GETFLAGS;
/// let ret = unsafe { nc::ioctl(fd, cmd, &mut attr as *mut i32 as *const _) };
/// assert!(ret.is_ok());
/// println!("attr: {}", attr);
///
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, path, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn ioctl(fd: i32, cmd: u32, arg: *const core::ffi::c_void) -> Result<i32, Errno> {
    let fd = fd as usize;
    let cmd = cmd as usize;
    let arg = arg as usize;
    syscall3(SYS_IOCTL, fd, cmd, arg).map(|ret| ret as i32)
}

/// Get I/O scheduling class and priority.
///
/// # Examples
///
/// ```
/// let ret = unsafe { nc::ioprio_get(nc::IOPRIO_WHO_PROCESS, nc::getpid()) };
/// assert!(ret.is_ok());
/// let prio = ret.unwrap();
/// let _prio_class = unsafe { nc::ioprio_prio_class(prio) };
/// let _prio_data = unsafe { nc::ioprio_prio_data(prio) };
/// ```
pub unsafe fn ioprio_get(which: i32, who: i32) -> Result<i32, Errno> {
    let which = which as usize;
    let who = who as usize;
    syscall2(SYS_IOPRIO_GET, which, who).map(|ret| ret as i32)
}

/// Set I/O scheduling class and priority.
///
/// See [ioprio](https://www.kernel.org/doc/Documentation/block/ioprio.txt)
///
/// # Examples
///
/// ```
/// // Change priority to lowest.
/// let new_prio_data = 7;
/// let new_prio = unsafe { nc::ioprio_prio_value(nc::IOPRIO_CLASS_IDLE, new_prio_data) };
/// let ret = unsafe { nc::ioprio_set(nc::IOPRIO_WHO_PROCESS, 0, new_prio) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn ioprio_set(which: i32, who: i32, ioprio: i32) -> Result<(), Errno> {
    let which = which as usize;
    let who = who as usize;
    let ioprio = ioprio as usize;
    syscall3(SYS_IOPRIO_SET, which, who, ioprio).map(drop)
}

/// Attempts to cancel an iocb previously passed to `io_submit`.
///
/// If the operation is successfully cancelled, the resulting event is
/// copied into the memory pointed to by `result` without being placed
/// into the completion queue and 0 is returned.
///
///
/// # Errors
/// - May fail with `-EFAULT` if any of the data structures pointed to are invalid.
/// - May fail with `-EINVAL` if `aio_context` specified by `ctx_id` is invalid.
/// - May fail with `-EAGAIN` if the iocb specified was not cancelled.
/// - Will fail with `-ENOSYS` if not implemented.
pub unsafe fn io_cancel(
    ctx_id: aio_context_t,
    iocb: &iocb_t,
    result: &mut io_event_t,
) -> Result<(), Errno> {
    let iocb_ptr = iocb as *const iocb_t as usize;
    let result_ptr = result as *mut io_event_t as usize;
    syscall3(SYS_IO_CANCEL, ctx_id, iocb_ptr, result_ptr).map(drop)
}

/// Destroy the `aio_context` specified.
///
/// May cancel any outstanding AIOs and block on completion.
///
/// Will fail with `-ENOSYS` if not implemented.
/// May fail with `-EINVAL` if the context pointed to is invalid.
///
/// # Examples
///
/// ```
/// use std::alloc::{alloc, Layout};
/// use std::ptr;
///
/// let mut ctx: nc::aio_context_t = 0;
/// let nr_events = 10;
///
/// let ret = unsafe { nc::io_setup(nr_events, &mut ctx) };
/// assert!(ret.is_ok());
///
/// let out_filename = "/tmp/nc-io-destroy";
/// let fd = unsafe {
///     nc::openat(
///         nc::AT_FDCWD,
///         out_filename,
///         nc::O_CREAT | nc::O_DIRECT | nc::O_WRONLY,
///         nc::S_IRUSR | nc::S_IWUSR,
///     )
/// };
/// assert!(fd.is_ok());
/// let fd = fd.unwrap();
///
/// let layout =
///     Layout::from_size_align(nc::PAGE_SIZE, nc::PAGE_SIZE).expect("Failed to create mem layout");
/// let ptr = unsafe { alloc(layout) };
/// if ptr.is_null() {
///     eprintln!("Failed to alloc aligned memory");
///     return;
/// }
/// let mut buf: Box<[u8]> = unsafe {
///     let slice = ptr::slice_from_raw_parts_mut(ptr, nc::PAGE_SIZE);
///     Box::from_raw(slice)
/// };
///
/// let msg = "hello Rust\n";
/// unsafe {
///     ptr::copy_nonoverlapping(msg.as_ptr(), buf.as_mut_ptr(), msg.len());
/// }
///
/// let mut iocb = Vec::with_capacity(1);
/// iocb.push(nc::iocb_t {
///     aio_data: buf.as_ptr() as u64,
///     aio_lio_opcode: nc::IOCB_CMD_PWRITE,
///     aio_fildes: fd as u32,
///     aio_buf: buf.as_ptr() as u64,
///     aio_nbytes: nc::PAGE_SIZE as u64,
///     ..Default::default()
/// });
///
/// let ret = unsafe { nc::io_submit(ctx, &iocb) };
/// if let Err(errno) = ret {
///     eprintln!("io_submit() failed, err: {}", nc::strerror(errno));
///     return;
/// }
///
/// let mut events = vec![nc::io_event_t::default(); 10];
/// let timeout = nc::timespec_t {
///     tv_sec: 1,
///     tv_nsec: 100,
/// };
///
/// let ret = unsafe { nc::io_getevents(ctx, 1, &mut events, Some(&timeout)) };
/// assert!(ret.is_ok());
/// let nread = ret.unwrap();
/// assert_eq!(nread, 1);
///
/// unsafe {
///     let _ret = nc::close(fd);
///     let _ret = nc::io_destroy(ctx);
/// }
/// ```
///
pub unsafe fn io_destroy(ctx_id: aio_context_t) -> Result<(), Errno> {
    syscall1(SYS_IO_DESTROY, ctx_id).map(drop)
}

/// Attempts to read at least `min_nr` events and up to nr events from
/// the completion queue for the `aio_context` specified by `ctx_id`.
///
/// If it succeeds, the number of read events is returned.
///
/// # Errors
///
/// - May fail with `-EINVAL` if `ctx_id` is invalid, if `min_nr` is out of range,
///   if `nr` is out of range, if `timeout` is out of range.
/// - May fail with `-EFAULT` if any of the memory specified is invalid.
/// - May return 0 or < `min_nr` if the timeout specified by timeout has elapsed
///   before sufficient events are available, where timeout == NULL
///   specifies an infinite timeout. Note that the timeout pointed to by timeout
///   is relative.
/// - Will fail with `-ENOSYS` if not implemented.
///
/// # Examples
///
/// ```
/// use std::alloc::{alloc, Layout};
/// use std::ptr;
///
/// let mut ctx: nc::aio_context_t = 0;
/// let nr_events = 10;
///
/// let ret = unsafe { nc::io_setup(nr_events, &mut ctx) };
/// assert!(ret.is_ok());
///
/// let out_filename = "/tmp/nc-io-getevents";
/// let fd = unsafe {
///     nc::openat(
///         nc::AT_FDCWD,
///         out_filename,
///         nc::O_CREAT | nc::O_DIRECT | nc::O_WRONLY,
///         nc::S_IRUSR | nc::S_IWUSR,
///     )
/// };
/// assert!(fd.is_ok());
/// let fd = fd.unwrap();
///
/// let layout =
///     Layout::from_size_align(nc::PAGE_SIZE, nc::PAGE_SIZE).expect("Failed to create mem layout");
/// let ptr = unsafe { alloc(layout) };
/// if ptr.is_null() {
///     eprintln!("Failed to alloc aligned memory");
///     return;
/// }
/// let mut buf: Box<[u8]> = unsafe {
///     let slice = ptr::slice_from_raw_parts_mut(ptr, nc::PAGE_SIZE);
///     Box::from_raw(slice)
/// };
///
/// let msg = "hello Rust\n";
/// unsafe {
///     ptr::copy_nonoverlapping(msg.as_ptr(), buf.as_mut_ptr(), msg.len());
/// }
///
/// let mut iocb = Vec::with_capacity(1);
/// iocb.push(nc::iocb_t {
///     aio_data: buf.as_ptr() as u64,
///     aio_lio_opcode: nc::IOCB_CMD_PWRITE,
///     aio_fildes: fd as u32,
///     aio_buf: buf.as_ptr() as u64,
///     aio_nbytes: nc::PAGE_SIZE as u64,
///     ..Default::default()
/// });
///
/// let ret = unsafe { nc::io_submit(ctx, &iocb) };
/// if let Err(errno) = ret {
///     eprintln!("io_submit() failed, err: {}", nc::strerror(errno));
///     return;
/// }
///
/// let mut events = vec![nc::io_event_t::default(); 10];
/// let timeout = nc::timespec_t {
///     tv_sec: 1,
///     tv_nsec: 100,
/// };
///
/// let ret = unsafe { nc::io_getevents(ctx, 1, &mut events, Some(&timeout)) };
/// assert!(ret.is_ok());
/// let nread = ret.unwrap();
/// assert_eq!(nread, 1);
///
/// unsafe {
///     let _ret = nc::close(fd);
///     let _ret = nc::io_destroy(ctx);
/// }
/// ```
///
pub unsafe fn io_getevents(
    ctx_id: aio_context_t,
    min_nr: usize,
    events: &mut [io_event_t],
    timeout: Option<&timespec_t>,
) -> Result<ssize_t, Errno> {
    let nr = events.len();
    let events_ptr = events.as_mut_ptr() as usize;
    let timeout_ptr = timeout.map_or(core::ptr::null::<timespec_t>() as usize, |timeout| {
        timeout as *const timespec_t as usize
    });
    syscall5(
        SYS_IO_GETEVENTS,
        ctx_id,
        min_nr,
        nr,
        events_ptr,
        timeout_ptr,
    )
    .map(|ret| ret as ssize_t)
}

/// Read asynchronous I/O events from the completion queue
///
/// # Examples
///
/// ```
/// use std::alloc::{alloc, Layout};
/// use std::ptr;
///
/// let mut ctx: nc::aio_context_t = 0;
/// let nr_events = 10;
///
/// let ret = unsafe { nc::io_setup(nr_events, &mut ctx) };
/// assert!(ret.is_ok());
///
/// let out_filename = "/tmp/nc-io-getevents";
/// let fd = unsafe {
///     nc::openat(
///         nc::AT_FDCWD,
///         out_filename,
///         nc::O_CREAT | nc::O_DIRECT | nc::O_WRONLY,
///         nc::S_IRUSR | nc::S_IWUSR,
///     )
/// };
/// assert!(fd.is_ok());
/// let fd = fd.unwrap();
///
/// let layout =
///     Layout::from_size_align(nc::PAGE_SIZE, nc::PAGE_SIZE).expect("Failed to create mem layout");
/// let ptr = unsafe { alloc(layout) };
/// if ptr.is_null() {
///     eprintln!("Failed to alloc aligned memory");
///     return;
/// }
/// let mut buf: Box<[u8]> = unsafe {
///     let slice = ptr::slice_from_raw_parts_mut(ptr, nc::PAGE_SIZE);
///     Box::from_raw(slice)
/// };
///
/// let msg = "hello Rust\n";
/// unsafe {
///     ptr::copy_nonoverlapping(msg.as_ptr(), buf.as_mut_ptr(), msg.len());
/// }
///
/// let mut iocb = Vec::with_capacity(1);
/// iocb.push(nc::iocb_t {
///     aio_data: buf.as_ptr() as u64,
///     aio_lio_opcode: nc::IOCB_CMD_PWRITE,
///     aio_fildes: fd as u32,
///     aio_buf: buf.as_ptr() as u64,
///     aio_nbytes: nc::PAGE_SIZE as u64,
///     ..Default::default()
/// });
///
/// let ret = unsafe { nc::io_submit(ctx, &iocb) };
/// if let Err(errno) = ret {
///     eprintln!("io_submit() failed, err: {}", nc::strerror(errno));
///     return;
/// }
///
/// let mut events = vec![nc::io_event_t::default(); 10];
/// let timeout = nc::timespec_t {
///     tv_sec: 1,
///     tv_nsec: 100,
/// };
///
/// let ret = unsafe { nc::io_pgetevents(ctx, 1, &mut events, Some(&timeout), None) };
/// assert!(ret.is_ok());
/// let nread = ret.unwrap();
/// assert_eq!(nread, 1);
///
/// unsafe {
///     let _ret = nc::close(fd);
///     let _ret = nc::io_destroy(ctx);
/// }
/// ```
///
pub unsafe fn io_pgetevents(
    ctx_id: aio_context_t,
    min_nr: isize,
    events: &mut [io_event_t],
    timeout: Option<&timespec_t>,
    sig: Option<&aio_sigset_t>,
) -> Result<i32, Errno> {
    use core::ptr::null;

    let min_nr = min_nr as usize;
    let nr = events.len();
    let events_ptr = events.as_mut_ptr() as usize;
    let timeout_ptr = timeout.map_or(null::<timespec_t>() as usize, |timeout| {
        timeout as *const timespec_t as usize
    });
    let sig_ptr = sig.map_or(null::<aio_sigset_t>() as usize, |sig| {
        sig as *const aio_sigset_t as usize
    });
    syscall6(
        SYS_IO_PGETEVENTS,
        ctx_id,
        min_nr,
        nr,
        events_ptr,
        timeout_ptr,
        sig_ptr,
    )
    .map(|ret| ret as i32)
}

/// Create an asynchronous I/O context.
///
/// Create an `aio_context` capable of receiving at least `nr_events`.
/// `ctx_id` must not point to an `aio_context` that already exists, and
/// must be initialized to 0 prior to the call.
///
/// On successful creation of the `aio_context`, `ctx_id` is filled in with
/// the resulting handle.
///
/// # Errors
///
/// - May fail with `-EINVAL` if `*ctx_id` is not initialized,
///   if the specified `nr_events` exceeds internal limits.
/// - May fail with `-EAGAIN` if the specified `nr_events` exceeds the user's limit
///   of available events.
/// - May fail with `-ENOMEM` if insufficient kernel resources are available.
/// - May fail with `-EFAULT` if an invalid pointer is passed for `ctx_id`.
/// - Will fail with `-ENOSYS` if not implemented.
///
/// # Examples
///
/// ```
/// use std::alloc::{alloc, Layout};
/// use std::ptr;
///
/// let mut ctx: nc::aio_context_t = 0;
/// let nr_events = 10;
///
/// let ret = unsafe { nc::io_setup(nr_events, &mut ctx) };
/// assert!(ret.is_ok());
///
/// let out_filename = "/tmp/nc-io-setup";
/// let fd = unsafe {
///     nc::openat(
///         nc::AT_FDCWD,
///         out_filename,
///         nc::O_CREAT | nc::O_DIRECT | nc::O_WRONLY,
///         nc::S_IRUSR | nc::S_IWUSR,
///     )
/// };
/// assert!(fd.is_ok());
/// let fd = fd.unwrap();
///
/// let layout =
///     Layout::from_size_align(nc::PAGE_SIZE, nc::PAGE_SIZE).expect("Failed to create mem layout");
/// let ptr = unsafe { alloc(layout) };
/// if ptr.is_null() {
///     eprintln!("Failed to alloc aligned memory");
///     return;
/// }
/// let mut buf: Box<[u8]> = unsafe {
///     let slice = ptr::slice_from_raw_parts_mut(ptr, nc::PAGE_SIZE);
///     Box::from_raw(slice)
/// };
///
/// let msg = "hello Rust\n";
/// unsafe {
///     ptr::copy_nonoverlapping(msg.as_ptr(), buf.as_mut_ptr(), msg.len());
/// }
///
/// let mut iocb = Vec::with_capacity(1);
/// iocb.push(nc::iocb_t {
///     aio_data: buf.as_ptr() as u64,
///     aio_lio_opcode: nc::IOCB_CMD_PWRITE,
///     aio_fildes: fd as u32,
///     aio_buf: buf.as_ptr() as u64,
///     aio_nbytes: nc::PAGE_SIZE as u64,
///     ..Default::default()
/// });
///
/// let ret = unsafe { nc::io_submit(ctx, &iocb) };
/// if let Err(errno) = ret {
///     eprintln!("io_submit() failed, err: {}", nc::strerror(errno));
///     return;
/// }
///
/// let mut events = vec![nc::io_event_t::default(); 10];
/// let timeout = nc::timespec_t {
///     tv_sec: 1,
///     tv_nsec: 100,
/// };
///
/// let ret = unsafe { nc::io_getevents(ctx, 1, &mut events, Some(&timeout)) };
/// assert!(ret.is_ok());
/// let nread = ret.unwrap();
/// assert_eq!(nread, 1);
///
/// unsafe {
///     let _ret = nc::close(fd);
///     let _ret = nc::io_destroy(ctx);
/// }
/// ```
///
pub unsafe fn io_setup(nr_events: u32, ctx_id: &mut aio_context_t) -> Result<(), Errno> {
    let nr_events = nr_events as usize;
    let ctx_id_ptr = ctx_id as *mut aio_context_t as usize;
    syscall2(SYS_IO_SETUP, nr_events, ctx_id_ptr).map(drop)
}

/// Queue the nr iocbs pointed to by `iocb` for processing.
///
/// Returns the number of iocbs queued.
///
/// # Errors
///
/// - May return `-EINVAL` if the `aio_context` specified by `ctx_id` is invalid,
///   if `nr` is < 0, if the `iocb` at `*iocbpp[0]` is not properly initialized,
///   if the operation specified is invalid for the file descriptor in the `iocb`.
/// - May fail with `-EFAULT` if any of the data structures point to invalid data.
/// - May fail with `-EBADF` if the file descriptor specified in the first `iocb` is invalid.
/// - May fail with `-EAGAIN` if insufficient resources are available to queue any iocbs.
/// - Will return 0 if nr is 0.
/// - Will fail with `-ENOSYS` if not implemented.
///
/// # Examples
///
/// ```
/// use std::alloc::{alloc, Layout};
/// use std::ptr;
///
/// let mut ctx: nc::aio_context_t = 0;
/// let nr_events = 10;
///
/// let ret = unsafe { nc::io_setup(nr_events, &mut ctx) };
/// assert!(ret.is_ok());
///
/// let out_filename = "/tmp/nc-io-submit";
/// let fd = unsafe {
///     nc::openat(
///         nc::AT_FDCWD,
///         out_filename,
///         nc::O_CREAT | nc::O_DIRECT | nc::O_WRONLY,
///         nc::S_IRUSR | nc::S_IWUSR,
///     )
/// };
/// assert!(fd.is_ok());
/// let fd = fd.unwrap();
///
/// let layout =
///     Layout::from_size_align(nc::PAGE_SIZE, nc::PAGE_SIZE).expect("Failed to create mem layout");
/// let ptr = unsafe { alloc(layout) };
/// if ptr.is_null() {
///     eprintln!("Failed to alloc aligned memory");
///     return;
/// }
/// let mut buf: Box<[u8]> = unsafe {
///     let slice = ptr::slice_from_raw_parts_mut(ptr, nc::PAGE_SIZE);
///     Box::from_raw(slice)
/// };
///
/// let msg = "hello Rust\n";
/// unsafe {
///     ptr::copy_nonoverlapping(msg.as_ptr(), buf.as_mut_ptr(), msg.len());
/// }
///
/// let mut iocb = Vec::with_capacity(1);
/// iocb.push(nc::iocb_t {
///     aio_data: buf.as_ptr() as u64,
///     aio_lio_opcode: nc::IOCB_CMD_PWRITE,
///     aio_fildes: fd as u32,
///     aio_buf: buf.as_ptr() as u64,
///     aio_nbytes: nc::PAGE_SIZE as u64,
///     ..Default::default()
/// });
///
/// let ret = unsafe { nc::io_submit(ctx, &iocb) };
/// if let Err(errno) = ret {
///     eprintln!("io_submit() failed, err: {}", nc::strerror(errno));
///     return;
/// }
///
/// let mut events = vec![nc::io_event_t::default(); 10];
/// let timeout = nc::timespec_t {
///     tv_sec: 1,
///     tv_nsec: 100,
/// };
///
/// let ret = unsafe { nc::io_getevents(ctx, 1, &mut events, Some(&timeout)) };
/// assert!(ret.is_ok());
/// let nread = ret.unwrap();
/// assert_eq!(nread, 1);
///
/// unsafe {
///     let _ret = nc::close(fd);
///     let _ret = nc::io_destroy(ctx);
/// }
/// ```
///
pub unsafe fn io_submit(ctx_id: aio_context_t, iocb: &[iocb_t]) -> Result<i32, Errno> {
    let nr = iocb.len();
    let iocb_ptr = core::ptr::addr_of!(iocb) as usize;
    syscall3(SYS_IO_SUBMIT, ctx_id, nr, iocb_ptr).map(|ret| ret as i32)
}

/// Initiate and/or complete asynchronous I/O
pub unsafe fn io_uring_enter(
    fd: i32,
    to_submit: u32,
    min_complete: u32,
    flags: u32,
    arg: *const core::ffi::c_void,
    arg_size: usize,
) -> Result<i32, Errno> {
    let fd = fd as usize;
    let to_submit = to_submit as usize;
    let min_complete = min_complete as usize;
    let flags = flags as usize;
    let arg = arg as usize;
    syscall6(
        SYS_IO_URING_ENTER,
        fd,
        to_submit,
        min_complete,
        flags,
        arg,
        arg_size,
    )
    .map(|ret| ret as i32)
}

/// Register files or user buffers for asynchronous I/O
pub unsafe fn io_uring_register(
    fd: u32,
    opcode: u32,
    arg: usize,
    nr_args: u32,
) -> Result<i32, Errno> {
    let fd = fd as usize;
    let opcode = opcode as usize;
    let nr_args = nr_args as usize;
    syscall4(SYS_IO_URING_REGISTER, fd, opcode, arg, nr_args).map(|ret| ret as i32)
}

/// Setup a context for performing asynchronous I/O.
///
/// Sets up a submission queue (SQ) and completion queue (CQ) with at least `entries`,
/// and returns a file descriptor which can be used to perform subsequent operations
/// on the `io_uring` instance.
/// The submission and completion queues are shared between userspace and the kernel,
/// which eliminates the need to copy data when initiating and completing I/O.
pub unsafe fn io_uring_setup(entries: u32, params: &mut io_uring_params_t) -> Result<u32, Errno> {
    let entries = entries as usize;
    let params_ptr = params as *mut io_uring_params_t as usize;
    syscall2(SYS_IO_URING_SETUP, entries, params_ptr).map(|ret| ret as u32)
}

/// Compare two processes to determine if they share a kernel resource.
pub unsafe fn kcmp(
    pid1: pid_t,
    pid2: pid_t,
    type_: i32,
    idx1: usize,
    idx2: usize,
) -> Result<i32, Errno> {
    let pid1 = pid1 as usize;
    let pid2 = pid2 as usize;
    let type_ = type_ as usize;
    syscall5(SYS_KCMP, pid1, pid2, type_, idx1, idx2).map(|ret| ret as i32)
}

/// Load a new kernel for later execution.
pub unsafe fn kexec_file_load<P: AsRef<Path>>(
    kernel_fd: i32,
    initrd_fd: i32,
    cmdline: P,
    flags: u32,
) -> Result<(), Errno> {
    let kernel_fd = kernel_fd as usize;
    let initrd_fd = initrd_fd as usize;
    let cmdline_len = cmdline.as_ref().len();
    let cmdline = CString::new(cmdline.as_ref());
    let cmdline_ptr = cmdline.as_ptr() as usize;
    let flags = flags as usize;
    syscall5(
        SYS_KEXEC_FILE_LOAD,
        kernel_fd,
        initrd_fd,
        cmdline_len,
        cmdline_ptr,
        flags,
    )
    .map(drop)
}

/// Load a new kernel for later execution.
pub unsafe fn kexec_load(
    entry: usize,
    segments: &mut [kexec_segment_t],
    flags: u32,
) -> Result<(), Errno> {
    let segments_ptr = segments.as_mut_ptr() as usize;
    let nr_segments = segments.len();
    let flags = flags as usize;
    syscall4(SYS_KEXEC_LOAD, entry, nr_segments, segments_ptr, flags).map(drop)
}

/// Manipulate the kernel's key management facility.
pub unsafe fn keyctl(
    operation: i32,
    arg2: usize,
    arg3: usize,
    arg4: usize,
    arg5: usize,
) -> Result<usize, Errno> {
    let operation = operation as usize;
    syscall5(SYS_KEYCTL, operation, arg2, arg3, arg4, arg5)
}

/// Send signal to a process.
///
/// # Examples
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

/// Get extended attribute value.
///
/// # Examples
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
///         attr_value.as_bytes(),
///         flags,
///     )
/// };
/// assert!(ret.is_ok());
/// let mut buf = [0_u8; 16];
/// let buf_len = buf.len();
/// let ret = unsafe { nc::lgetxattr(path, attr_name, &mut buf) };
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
    value: &mut [u8],
) -> Result<ssize_t, Errno> {
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    let name = CString::new(name.as_ref());
    let name_ptr = name.as_ptr() as usize;
    let value_ptr = value.as_mut_ptr() as usize;
    let size = value.len();
    syscall4(SYS_LGETXATTR, filename_ptr, name_ptr, value_ptr, size).map(|ret| ret as ssize_t)
}

/// Make a new name for a file.
///
/// # Examples
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

/// Listen for connections on a socket.
///
/// # Examples
///
/// ```
/// use nc::Errno;
/// use std::mem::{size_of, transmute};
/// use std::thread;
///
/// const SERVER_PORT: u16 = 18086;
///
/// #[must_use]
/// #[inline]
/// const fn htons(host: u16) -> u16 {
///     host.to_be()
/// }
///
/// fn main() -> Result<(), Errno> {
///     let listen_fd = unsafe { nc::socket(nc::AF_INET, nc::SOCK_STREAM, 0)? };
///     println!("listen fd: {listen_fd}");
///
///     let addr = nc::sockaddr_in_t {
///         sin_family: nc::AF_INET as nc::sa_family_t,
///         sin_port: htons(SERVER_PORT),
///         sin_addr: nc::in_addr_t {
///             s_addr: nc::INADDR_ANY as u32,
///         },
///         ..Default::default()
///     };
///     println!("addr: {addr:?}");
///
///     let ret = unsafe {
///         let addr_alias = transmute::<&nc::sockaddr_in_t, &nc::sockaddr_t>(&addr);
///         nc::bind(listen_fd, addr_alias, size_of::<nc::sockaddr_in_t>() as u32)
///     };
///     assert!(ret.is_ok());
///
///     // Start worker thread
///     thread::spawn(|| {
///         println!("worker thread started");
///         let socket_fd = unsafe { nc::socket(nc::AF_INET, nc::SOCK_STREAM, 0) };
///         assert!(socket_fd.is_ok());
///         if let Ok(socket_fd) = socket_fd {
///             let addr = nc::sockaddr_in_t {
///                 sin_family: nc::AF_INET as nc::sa_family_t,
///                 sin_port: htons(SERVER_PORT),
///                 sin_addr: nc::in_addr_t {
///                     s_addr: nc::INADDR_ANY as u32,
///                 },
///                 ..Default::default()
///             };
///             unsafe {
///                 let addr_alias = transmute::<&nc::sockaddr_in_t, &nc::sockaddr_t>(&addr);
///                 let ret = nc::connect(socket_fd, addr_alias, size_of::<nc::sockaddr_in_t>() as u32);
///                 assert_eq!(ret, Ok(()));
///             }
///         } else {
///             eprintln!("Failed to create socket");
///         }
///     });
///
///     unsafe {
///         nc::listen(listen_fd, nc::SOCK_STREAM)?;
///     }
///
///     let conn_fd = unsafe {
///         nc::accept4(
///             listen_fd,
///             None,
///             None,
///             nc::SOCK_CLOEXEC,
///         )?
///     };
///     println!("conn_fd: {conn_fd}");
///
///     unsafe {
///         nc::close(listen_fd)?;
///     }
///
///     Ok(())
/// }
/// ```
pub unsafe fn listen(sockfd: i32, backlog: i32) -> Result<(), Errno> {
    let sockfd = sockfd as usize;
    let backlog = backlog as usize;
    syscall2(SYS_LISTEN, sockfd, backlog).map(drop)
}

pub unsafe fn listmount(req: &mnt_id_req_t, mnt_ids: &mut [u64], flags: u32) -> Result<(), Errno> {
    let req_ptr = req as *const mnt_id_req_t as usize;
    let mnt_ids_ptr = mnt_ids.as_mut_ptr() as usize;
    let nr_mnt_ids = mnt_ids.len();
    let flags = flags as usize;
    syscall4(SYS_LISTMOUNT, req_ptr, mnt_ids_ptr, nr_mnt_ids, flags).map(drop)
}

/// List extended attribute names.
///
/// # Examples
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
///         attr_value.as_bytes(),
///         flags,
///     )
/// };
/// assert!(ret.is_ok());
/// let mut buf = [0_u8; 16];
/// let buf_len = buf.len();
/// let ret = unsafe { nc::listxattr(path, &mut buf) };
/// let attr_len = ret.unwrap() as usize;
/// assert_eq!(&buf[..attr_len - 1], attr_name.as_bytes());
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, path, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn listxattr<P: AsRef<Path>>(filename: P, value: &mut [u8]) -> Result<ssize_t, Errno> {
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    let value_ptr = value.as_mut_ptr() as usize;
    let size = value.len();
    syscall3(SYS_LISTXATTR, filename_ptr, value_ptr, size).map(|ret| ret as ssize_t)
}

/// List extended attribute names.
///
/// # Examples
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
///         attr_value.as_bytes(),
///         flags,
///     )
/// };
/// assert!(ret.is_ok());
/// let mut buf = [0_u8; 16];
/// let buf_len = buf.len();
/// let ret = unsafe { nc::llistxattr(path, &mut buf) };
/// let attr_len = ret.unwrap() as usize;
/// assert_eq!(&buf[..attr_len - 1], attr_name.as_bytes());
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, path, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn llistxattr<P: AsRef<Path>>(filename: P, value: &mut [u8]) -> Result<ssize_t, Errno> {
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    let value_ptr = value.as_mut_ptr() as usize;
    let size = value.len();
    syscall3(SYS_LLISTXATTR, filename_ptr, value_ptr, size).map(|ret| ret as ssize_t)
}

/// Return a directory entry's path.
pub unsafe fn lookup_dcookie(cookie: u64, buf: &mut [u8]) -> Result<ssize_t, Errno> {
    let cookie = cookie as usize;
    let buf_ptr = buf.as_mut_ptr() as usize;
    let buf_len = buf.len();
    syscall3(SYS_LOOKUP_DCOOKIE, cookie, buf_ptr, buf_len).map(|ret| ret as ssize_t)
}

/// Remove an extended attribute.
///
/// # Examples
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
///         attr_value.as_bytes(),
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
/// # Examples
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
/// # Examples
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
///         attr_value.as_bytes(),
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
    value: &[u8],
    flags: i32,
) -> Result<(), Errno> {
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    let name = CString::new(name.as_ref());
    let name_ptr = name.as_ptr() as usize;
    let value_ptr = value.as_ptr() as usize;
    let size = value.len();
    let flags = flags as usize;
    syscall5(
        SYS_LSETXATTR,
        filename_ptr,
        name_ptr,
        value_ptr,
        size,
        flags,
    )
    .map(drop)
}

/// Give advice about use of memory.
///
/// # Examples
///
/// ```
/// use std::ptr;
///
/// // Initialize an anonymous mapping with 4 pages.
/// let map_length = 4 * nc::PAGE_SIZE;
/// let ret = unsafe {
///     nc::mmap(
///         ptr::null(),
///         map_length,
///         nc::PROT_READ | nc::PROT_WRITE,
///         nc::MAP_PRIVATE | nc::MAP_ANONYMOUS,
///         -1,
///         0,
///     )
/// };
/// assert!(ret.is_ok());
/// let addr = ret.unwrap();
///
/// // Notify kernel that the third page will be accessed.
/// let ret = unsafe { nc::madvise(addr.wrapping_add(2 * nc::PAGE_SIZE), nc::PAGE_SIZE, nc::MADV_WILLNEED) };
/// assert!(ret.is_ok());
///
/// let ret = unsafe { nc::munmap(addr, map_length) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn madvise(
    addr: *const core::ffi::c_void,
    len: size_t,
    advice: i32,
) -> Result<(), Errno> {
    let addr = addr as usize;
    let advice = advice as usize;
    syscall3(SYS_MADVISE, addr, len, advice).map(drop)
}

pub unsafe fn map_shadow_stack(addr: usize, size: usize, flags: u32) -> Result<usize, Errno> {
    let flags = flags as usize;
    syscall3(SYS_MAP_SHADOW_STACK, addr, size, flags)
}

/// Set memory policy for a memory range.
pub unsafe fn mbind(
    start: *const core::ffi::c_void,
    len: usize,
    mode: i32,
    nmask: &[usize],
    maxnode: usize,
    flags: u32,
) -> Result<(), Errno> {
    let start = start as usize;
    let mode = mode as usize;
    let nmask = nmask.as_ptr() as usize;
    let flags = flags as usize;
    syscall6(SYS_MBIND, start, len, mode, nmask, maxnode, flags).map(drop)
}

/// Issue memory barriers on a set of threads.
///
/// @cmd:   Takes command values defined in enum `membarrier_cmd`.
/// @flags: Currently needs to be 0. For future extensions.
///
/// If this system call is not implemented, `-ENOSYS` is returned. If the
/// command specified does not exist, not available on the running
/// kernel, or if the command argument is invalid, this system call
/// returns `-EINVAL`. For a given command, with flags argument set to 0,
/// this system call is guaranteed to always return the same value until
/// reboot.
///
/// All memory accesses performed in program order from each targeted thread
/// is guaranteed to be ordered with respect to `sys_membarrier()`. If we use
/// the semantic `barrier()` to represent a compiler barrier forcing memory
/// accesses to be performed in program order across the barrier, and
/// `smp_mb()` to represent explicit memory barriers forcing full memory
/// ordering across the barrier, we have the following ordering table for
/// each pair of `barrier()`, `sys_membarrier()` and `smp_mb()`:
///
/// The pair ordering is detailed as (O: ordered, X: not ordered):
///
/// ```text
///                        barrier()   smp_mb() sys_membarrier()
///        barrier()          X           X            O
///        smp_mb()           X           O            O
///        sys_membarrier()   O           O            O
/// ```
pub unsafe fn membarrier(cmd: i32, flags: u32, cpuid: i32) -> Result<i32, Errno> {
    let cmd = cmd as usize;
    let flags = flags as usize;
    let cpuid = cpuid as usize;
    syscall3(SYS_MEMBARRIER, cmd, flags, cpuid).map(|ret| ret as i32)
}

/// Create an anonymous file.
pub unsafe fn memfd_create<P: AsRef<Path>>(name: P, flags: u32) -> Result<i32, Errno> {
    let name = CString::new(name.as_ref());
    let name_ptr = name.as_ptr() as usize;
    let flags = flags as usize;
    syscall2(SYS_MEMFD_CREATE, name_ptr, flags).map(|ret| ret as i32)
}

/// create an anonymous RAM-based file to access secret memory regions.
pub unsafe fn memfd_secret(flags: u32) -> Result<i32, Errno> {
    let flags = flags as usize;
    syscall1(SYS_MEMFD_SECRET, flags).map(|ret| ret as i32)
}

/// Move all pages in a process to another set of nodes
pub unsafe fn migrate_pages(
    pid: pid_t,
    max_node: usize,
    old_nodes: &[usize],
    new_nodes: &[usize],
) -> Result<isize, Errno> {
    let pid = pid as usize;
    let old_nodes = old_nodes.as_ptr() as usize;
    let new_nodes = new_nodes.as_ptr() as usize;
    syscall4(SYS_MIGRATE_PAGES, pid, max_node, old_nodes, new_nodes).map(|ret| ret as isize)
}

/// `mincore()` returns the memory residency status of the pages in the
/// current process's address space specified by `[addr, addr + len)`.
///
/// The status is returned in a vector of bytes.  The least significant
/// bit of each byte is 1 if the referenced page is in memory, otherwise
/// it is zero.
///
/// Because the status of a page can change after `mincore()` checks it
/// but before it returns to the application, the returned vector may
/// contain stale information.  Only locked pages are guaranteed to
/// remain in memory.
///
/// return values:
///  zero    - success
///  -EFAULT - vec points to an illegal address
///  -EINVAL - addr is not a multiple of `PAGE_SIZE`
///  -ENOMEM - Addresses in the range `[addr, addr + len]` are
/// invalid for the address space of this process, or specify one or
/// more pages which are not currently mapped
///  -EAGAIN - A kernel resource was temporarily unavailable.
pub unsafe fn mincore(
    start: *const core::ffi::c_void,
    len: size_t,
    vec: &mut [u8],
) -> Result<(), Errno> {
    let start = start as usize;
    let vec_ptr = vec.as_ptr() as usize;
    syscall3(SYS_MINCORE, start, len, vec_ptr).map(drop)
}

/// Create a directory.
///
/// # Examples
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

/// Create a special or ordinary file.
///
/// # Examples
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
/// # Examples
///
/// ```
/// let mut passwd_buf = vec![0; 64];
/// let ret = unsafe { nc::mlock(passwd_buf.as_ptr() as *const _, passwd_buf.len()) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn mlock(addr: *const core::ffi::c_void, len: size_t) -> Result<(), Errno> {
    let addr = addr as usize;
    syscall2(SYS_MLOCK, addr, len).map(drop)
}

/// Lock memory.
///
/// # Examples
///
/// ```
/// let mut passwd_buf = vec![0; 64];
/// let ret = unsafe { nc::mlock2(passwd_buf.as_ptr() as *const _, passwd_buf.len(), nc::MCL_CURRENT) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn mlock2(addr: *const core::ffi::c_void, len: size_t, flags: i32) -> Result<(), Errno> {
    let addr = addr as usize;
    let flags = flags as usize;
    syscall3(SYS_MLOCK2, addr, len, flags).map(drop)
}

/// Lock memory.
///
/// # Examples
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
/// # Examples
///
/// ```
/// use std::{mem, ptr};
/// use std::ffi::c_void;
///
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
///         ptr::null(),
///         map_length,
///         nc::PROT_READ,
///         nc::MAP_PRIVATE,
///         fd,
///         pa_offset as nc::off_t,
///     )
/// };
/// assert!(addr.is_ok());
/// let addr: *const c_void = addr.unwrap();
///
/// let stdout = 1;
/// // Create the "fat pointer".
/// let buf = unsafe {
///     mem::transmute::<(usize, usize), &[u8]>((addr as usize + offset - pa_offset, length))
/// };
/// let n_write = unsafe { nc::write(stdout, buf) };
/// assert!(n_write.is_ok());
/// assert_eq!(n_write, Ok(length as nc::ssize_t));
/// let ret = unsafe { nc::munmap(addr, map_length) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn mmap(
    start: *const core::ffi::c_void,
    len: size_t,
    prot: i32,
    flags: i32,
    fd: i32,
    offset: off_t,
) -> Result<*const core::ffi::c_void, Errno> {
    let start = start as usize;
    let prot = prot as usize;
    let flags = flags as usize;
    let fd = fd as usize;
    let offset = offset as usize;
    syscall6(SYS_MMAP, start, len, prot, flags, fd, offset)
        .map(|ret| ret as *const core::ffi::c_void)
}

/// Mount filesystem.
///
/// # Examples
///
/// ```
/// let target_dir = "/tmp/nc-mount";
/// let ret = unsafe { nc::mkdirat(nc::AT_FDCWD, target_dir, 0o755) };
/// assert!(ret.is_ok());
///
/// let src_dir = "/etc";
/// let fs_type = "";
/// let mount_flags = nc::MS_BIND | nc::MS_RDONLY;
/// let data = std::ptr::null_mut();
/// let ret = unsafe { nc::mount(src_dir, target_dir, fs_type, mount_flags, data) };
/// assert!(ret.is_err());
/// assert_eq!(ret, Err(nc::EPERM));
///
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, target_dir, nc::AT_REMOVEDIR) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn mount<P: AsRef<Path>>(
    dev_name: P,
    dir_name: P,
    fs_type: P,
    flags: u32,
    data_page: *mut core::ffi::c_void,
) -> Result<(), Errno> {
    let dev_name = CString::new(dev_name.as_ref());
    let dev_name_ptr = dev_name.as_ptr() as usize;
    let dir_name = CString::new(dir_name.as_ref());
    let dir_name_ptr = dir_name.as_ptr() as usize;
    let fs_type = CString::new(fs_type.as_ref());
    let fs_type_ptr = fs_type.as_ptr() as usize;
    let flags = flags as usize;
    let data_page = data_page as usize;
    syscall5(
        SYS_MOUNT,
        dev_name_ptr,
        dir_name_ptr,
        fs_type_ptr,
        flags,
        data_page,
    )
    .map(drop)
}

/// Move a mount from one place to another.
///
/// In combination with `fsopen()/fsmount()` this is used to install a new mount
/// and in combination with `open_tree(OPEN_TREE_CLONE [| AT_RECURSIVE])`
/// it can be used to copy a mount subtree.
///
/// Note the flags value is a combination of `MOVE_MOUNT_*` flags.
pub unsafe fn move_mount<P: AsRef<Path>>(
    from_dfd: i32,
    from_pathname: P,
    to_dfd: i32,
    to_pathname: P,
    flags: u32,
) -> Result<i32, Errno> {
    let from_dfd = from_dfd as usize;
    let from_pathname = CString::new(from_pathname.as_ref());
    let from_pathname_ptr = from_pathname.as_ptr() as usize;
    let to_dfd = to_dfd as usize;
    let to_pathname = CString::new(to_pathname.as_ref());
    let to_pathname_ptr = to_pathname.as_ptr() as usize;
    let flags = flags as usize;
    syscall5(
        SYS_MOVE_MOUNT,
        from_dfd,
        from_pathname_ptr,
        to_dfd,
        to_pathname_ptr,
        flags,
    )
    .map(|ret| ret as i32)
}

/// Move individual pages of a process to another node
pub unsafe fn move_pages(
    pid: pid_t,
    nr_pages: usize,
    pages: usize,
    nodes: *const i32,
    status: Option<&mut i32>,
    flags: u32,
) -> Result<(), Errno> {
    let pid = pid as usize;
    let nodes_ptr = nodes as usize;
    let status = status.map_or(core::ptr::null_mut::<i32>() as usize, |status| {
        status as *mut i32 as usize
    });
    // NOTE(Shaohua): Type of flags is i32 in kernel.
    let flags = flags as usize;
    syscall6(
        SYS_MOVE_PAGES,
        pid,
        nr_pages,
        pages,
        nodes_ptr,
        status,
        flags,
    )
    .map(drop)
}

/// Set protection on a region of memory.
///
/// # Examples
///
/// ```
/// // Initialize an anonymous mapping with 4 pages.
/// let map_length = 4 * nc::PAGE_SIZE;
/// let addr = unsafe {
///     nc::mmap(
///         std::ptr::null(),
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
/// let ret = unsafe { nc::mprotect(addr.wrapping_add(2 * nc::PAGE_SIZE), nc::PAGE_SIZE, nc::PROT_READ) };
/// assert!(ret.is_ok());
///
/// let ret = unsafe { nc::munmap(addr, map_length) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn mprotect(
    addr: *const core::ffi::c_void,
    len: size_t,
    prot: i32,
) -> Result<(), Errno> {
    let addr = addr as usize;
    let prot = prot as usize;
    syscall3(SYS_MPROTECT, addr, len, prot).map(drop)
}

/// Get/set message queue attributes
///
/// # Examples
///
/// ```
/// let name = "nc-mq-getsetattr";
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
///
/// let mut old_attr = nc::mq_attr_t::default();
/// let ret = unsafe { nc::mq_getsetattr(mq_id, None, Some(&mut old_attr)) };
/// assert!(ret.is_ok());
/// println!("old attr: {:?}", old_attr);
///
/// let ret = unsafe { nc::close(mq_id) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::mq_unlink(name) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn mq_getsetattr(
    mqdes: mqd_t,
    new_attr: Option<&mq_attr_t>,
    old_attr: Option<&mut mq_attr_t>,
) -> Result<mqd_t, Errno> {
    let mqdes = mqdes as usize;
    let new_attr_ptr = new_attr.map_or(core::ptr::null::<mq_attr_t>() as usize, |new_attr| {
        new_attr as *const mq_attr_t as usize
    });
    let old_attr_ptr = old_attr.map_or(core::ptr::null_mut::<mq_attr_t>() as usize, |old_attr| {
        old_attr as *mut mq_attr_t as usize
    });
    syscall3(SYS_MQ_GETSETATTR, mqdes, new_attr_ptr, old_attr_ptr).map(|ret| ret as mqd_t)
}

/// Register for notification when a message is available
pub unsafe fn mq_notify(mqdes: mqd_t, notification: Option<&sigevent_t>) -> Result<(), Errno> {
    let mqdes = mqdes as usize;
    let notification_ptr = notification
        .map_or(core::ptr::null::<sigevent_t>() as usize, |notification| {
            notification as *const sigevent_t as usize
        });
    syscall2(SYS_MQ_NOTIFY, mqdes, notification_ptr).map(drop)
}

/// Open a POSIX message queue.
///
/// # Examples
///
/// ```
/// let name = "nc-posix-mq";
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
pub unsafe fn mq_open<P: AsRef<Path>>(
    name: P,
    oflag: i32,
    mode: umode_t,
    attr: Option<&mut mq_attr_t>,
) -> Result<mqd_t, Errno> {
    let name = CString::new(name.as_ref());
    let name_ptr = name.as_ptr() as usize;
    let oflag = oflag as usize;
    let mode = mode as usize;
    let attr_ptr = attr.map_or(core::ptr::null_mut::<mq_attr_t>() as usize, |attr| {
        attr as *mut mq_attr_t as usize
    });
    syscall4(SYS_MQ_OPEN, name_ptr, oflag, mode, attr_ptr).map(|ret| ret as mqd_t)
}

/// Receive a message from a message queue
///
/// # Examples
///
/// ```
/// let name = "nc-mq-timedreceive";
/// let ret = unsafe {
///     nc::mq_open(
///         name,
///         nc::O_CREAT | nc::O_RDWR | nc::O_EXCL,
///         (nc::S_IRUSR | nc::S_IWUSR) as nc::umode_t,
///         None,
///     )
/// };
/// assert!(ret.is_ok());
/// let mq_id = ret.unwrap();
///
/// let mut attr = nc::mq_attr_t::default();
/// let ret = unsafe { nc::mq_getsetattr(mq_id, None, Some(&mut attr)) };
/// assert!(ret.is_ok());
/// println!("attr: {:?}", attr);
///
/// let msg = "Hello, Rust";
/// let prio = 42;
/// let timeout = nc::timespec_t {
///     tv_sec: 1,
///     tv_nsec: 0,
/// };
/// let ret = unsafe { nc::mq_timedsend(mq_id, msg.as_bytes(), prio, &timeout) };
/// assert!(ret.is_ok());
///
/// let ret = unsafe { nc::mq_getsetattr(mq_id, None, Some(&mut attr)) };
/// assert!(ret.is_ok());
/// assert_eq!(attr.mq_curmsgs, 1);
///
/// let mut buf = vec![0_u8; attr.mq_msgsize as usize];
/// let mut recv_prio = 0;
/// let read_timeout = nc::timespec_t {
///     tv_sec: 1,
///     tv_nsec: 0,
/// };
/// let ret = unsafe { nc::mq_timedreceive(mq_id, &mut buf, &mut recv_prio, &read_timeout) };
/// if let Err(errno) = ret {
///     eprintln!("mq_timedreceive() error: {}", nc::strerror(errno));
/// }
/// assert!(ret.is_ok());
/// let n_read = ret.unwrap() as usize;
/// assert_eq!(n_read, msg.len());
///
/// let ret = unsafe { nc::close(mq_id) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::mq_unlink(name) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn mq_timedreceive(
    mqdes: mqd_t,
    msg: &mut [u8],
    msg_prio: &mut u32,
    abs_timeout: &timespec_t,
) -> Result<ssize_t, Errno> {
    let mqdes = mqdes as usize;
    let msg_ptr = msg.as_mut_ptr() as usize;
    let msg_len = msg.len();
    let msg_prio = msg_prio as *mut u32 as usize;
    let abs_timeout_ptr = abs_timeout as *const timespec_t as usize;
    syscall5(
        SYS_MQ_TIMEDRECEIVE,
        mqdes,
        msg_ptr,
        msg_len,
        msg_prio,
        abs_timeout_ptr,
    )
    .map(|ret| ret as ssize_t)
}

/// Send message to a message queue.
///
/// # Examples
///
/// ```
/// let name = "nc-mq-timedsend";
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
///
/// let mut attr = nc::mq_attr_t::default();
/// let ret = unsafe { nc::mq_getsetattr(mq_id, None, Some(&mut attr)) };
/// assert!(ret.is_ok());
///
/// let msg = "Hello, Rust";
/// let prio = 0;
/// let timeout = nc::timespec_t {
///     tv_sec: 1,
///     tv_nsec: 0,
/// };
/// let ret = unsafe { nc::mq_timedsend(mq_id, msg.as_bytes(), prio, &timeout) };
/// assert!(ret.is_ok());
///
/// let ret = unsafe { nc::mq_getsetattr(mq_id, None, Some(&mut attr)) };
/// assert!(ret.is_ok());
/// assert_eq!(attr.mq_curmsgs, 1);
///
/// let ret = unsafe { nc::close(mq_id) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::mq_unlink(name) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn mq_timedsend(
    mqdes: mqd_t,
    msg: &[u8],
    msg_prio: u32,
    abs_timeout: &timespec_t,
) -> Result<(), Errno> {
    let mqdes = mqdes as usize;
    let msg = CString::new(msg);
    let msg_ptr = msg.as_ptr() as usize;
    let msg_len = msg.len();
    let msg_prio = msg_prio as usize;
    let abs_timeout_ptr = abs_timeout as *const timespec_t as usize;
    syscall5(
        SYS_MQ_TIMEDSEND,
        mqdes,
        msg_ptr,
        msg_len,
        msg_prio,
        abs_timeout_ptr,
    )
    .map(drop)
}

/// Remove a message queue.
///
/// # Examples
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
///
/// # Examples
///
/// ```
/// use std::ffi::c_void;
/// use std::ptr;
///
/// // Initialize an anonymous mapping with 2 pages.
/// let map_length = 2 * nc::PAGE_SIZE;
///
/// let addr = unsafe {
///     nc::mmap(
///         ptr::null(),
///         map_length,
///         nc::PROT_READ | nc::PROT_WRITE,
///         nc::MAP_PRIVATE | nc::MAP_ANONYMOUS,
///         -1,
///         0,
///     )
/// };
/// assert!(addr.is_ok());
/// let addr: *const c_void = addr.unwrap();
///
/// let new_map_length = 4 * nc::PAGE_SIZE;
/// let new_addr = unsafe {
///     nc::mremap(
///         addr,
///         map_length,
///         new_map_length,
///         nc::MREMAP_MAYMOVE,
///         ptr::null(),
///     )
/// };
/// if let Err(errno) = new_addr {
///     eprintln!("mremap() err: {}", nc::strerror(errno));
/// }
/// assert!(new_addr.is_ok());
/// let new_addr: *const c_void = new_addr.unwrap();
///
/// let ret = unsafe { nc::munmap(new_addr, map_length) };
/// assert!(ret.is_ok());
/// unsafe { nc::exit(0) };
/// ```
pub unsafe fn mremap(
    addr: *const core::ffi::c_void,
    old_len: size_t,
    new_len: size_t,
    flags: i32,
    new_addr: *const core::ffi::c_void,
) -> Result<*const core::ffi::c_void, Errno> {
    let addr = addr as usize;
    let flags = flags as usize;
    let new_addr = new_addr as usize;
    syscall5(SYS_MREMAP, addr, old_len, new_len, flags, new_addr)
        .map(|ret| ret as *const core::ffi::c_void)
}

/// Seals the VM's metadata from selected syscalls.
///
/// - addr/len: VM address range.
pub unsafe fn mseal(
    start: *const core::ffi::c_void,
    len: size_t,
    flags: usize,
) -> Result<(), Errno> {
    let start = start as usize;
    syscall3(SYS_MSEAL, start, len, flags).map(drop)
}

/// System V message control operations.
///
/// # Examples
///
/// ```
/// let key = nc::IPC_PRIVATE;
/// let flags = nc::IPC_CREAT | nc::IPC_EXCL | (nc::S_IRUSR | nc::S_IWUSR) as i32;
/// let ret = unsafe { nc::msgget(key, flags) };
/// assert!(ret.is_ok());
/// let msq_id = ret.unwrap();
///
/// let mut buf = nc::msqid_ds_t::default();
/// let ret = unsafe { nc::msgctl(msq_id, nc::IPC_RMID, &mut buf) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn msgctl(msq_id: i32, cmd: i32, buf: &mut msqid_ds_t) -> Result<i32, Errno> {
    let msq_id = msq_id as usize;
    let cmd = cmd as usize;
    let buf_ptr = buf as *mut msqid_ds_t as usize;
    syscall3(SYS_MSGCTL, msq_id, cmd, buf_ptr).map(|ret| ret as i32)
}

/// Get a System V message queue identifier.
///
/// # Examples
///
/// ```
/// let key = nc::IPC_PRIVATE;
/// let flags = nc::IPC_CREAT | nc::IPC_EXCL | (nc::S_IRUSR | nc::S_IWUSR) as i32;
/// let ret = unsafe { nc::msgget(key, flags) };
/// assert!(ret.is_ok());
/// let msq_id = ret.unwrap();
///
/// let mut buf = nc::msqid_ds_t::default();
/// let ret = unsafe { nc::msgctl(msq_id, nc::IPC_RMID, &mut buf) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn msgget(key: key_t, msg_flag: i32) -> Result<i32, Errno> {
    let key = key as usize;
    let msg_flag = msg_flag as usize;
    syscall2(SYS_MSGGET, key, msg_flag).map(|ret| ret as i32)
}

/// Receive messages from a System V message queue.
///
/// # Examples
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
///     let ret = unsafe { nc::msgsnd(msq_id, &client_msg as *const Message as *const _, msg_len, 0) };
///     assert!(ret.is_ok());
///
///     // Read from message queue.
///     let mut recv_msg = Message::default();
///     let ret = unsafe {
///         nc::msgrcv(
///             msq_id,
///             &mut recv_msg as *mut Message as *mut _,
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
    msq_id: i32,
    msgq: *mut core::ffi::c_void,
    msg_size: size_t,
    msg_type: isize,
    msg_flag: i32,
) -> Result<ssize_t, Errno> {
    let msq_id = msq_id as usize;
    let msgq = msgq as usize;
    let msg_type = msg_type as usize;
    let msg_flag = msg_flag as usize;
    syscall5(SYS_MSGRCV, msq_id, msgq, msg_size, msg_type, msg_flag).map(|ret| ret as ssize_t)
}

/// Append the message to a System V message queue.
///
/// # Examples
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
///     let ret = unsafe { nc::msgsnd(msq_id, &client_msg as *const Message as *const _, msg_len, 0) };
///     assert!(ret.is_ok());
///
///     // Read from message queue.
///     let mut recv_msg = Message::default();
///     let ret = unsafe {
///         nc::msgrcv(
///             msq_id,
///             &mut recv_msg as *mut Message as *mut _,
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
pub unsafe fn msgsnd(
    msq_id: i32,
    msgq: *const core::ffi::c_void,
    msg_size: size_t,
    msg_flag: i32,
) -> Result<(), Errno> {
    let msq_id = msq_id as usize;
    let msgq = msgq as usize;
    let msg_flag = msg_flag as usize;
    syscall4(SYS_MSGSND, msq_id, msgq, msg_size, msg_flag).map(drop)
}

/// Synchronize a file with memory map.
pub unsafe fn msync(addr: *const core::ffi::c_void, len: size_t, flags: i32) -> Result<(), Errno> {
    let addr = addr as usize;
    let flags = flags as usize;
    syscall3(SYS_MSYNC, addr, len, flags).map(drop)
}

/// Unlock memory.
///
/// # Examples
///
/// ```
/// let mut passwd_buf = vec![0; 64];
/// let addr = passwd_buf.as_ptr() as *const _;
/// let ret = unsafe { nc::mlock2(addr, passwd_buf.len(), nc::MCL_CURRENT) };
/// for i in 0..passwd_buf.len() {
///   passwd_buf[i] = i as u8;
/// }
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::munlock(addr, passwd_buf.len()) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn munlock(addr: *const core::ffi::c_void, len: size_t) -> Result<(), Errno> {
    let addr = addr as usize;
    syscall2(SYS_MUNLOCK, addr, len).map(drop)
}

/// Unlock memory.
///
/// # Examples
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
/// # Examples
///
/// ```
/// use std::{mem, ptr};
///
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
///         ptr::null(),
///         map_length,
///         nc::PROT_READ,
///         nc::MAP_PRIVATE,
///         fd,
///         pa_offset as nc::off_t,
///     )
/// };
/// assert!(addr.is_ok());
/// let addr = addr.unwrap();
/// let stdout = 1;
///
/// // Create the "fat pointer".
/// let buf = unsafe {
///     mem::transmute::<(usize, usize), &[u8]>((addr as usize + offset - pa_offset, length))
/// };
/// let n_write = unsafe { nc::write(stdout, buf) };
/// assert!(n_write.is_ok());
/// assert_eq!(n_write, Ok(length as nc::ssize_t));
/// let ret = unsafe { nc::munmap(addr, map_length) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn munmap(addr: *const core::ffi::c_void, len: size_t) -> Result<(), Errno> {
    let addr = addr as usize;
    syscall2(SYS_MUNMAP, addr, len).map(drop)
}

/// Obtain handle for a filename
pub unsafe fn name_to_handle_at<P: AsRef<Path>>(
    dfd: i32,
    filename: P,
    handle: &mut file_handle_t,
    mount_id: &mut i32,
    flags: i32,
) -> Result<(), Errno> {
    let dfd = dfd as usize;
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    let handle_ptr = handle as *mut file_handle_t as usize;
    let mount_id_ptr = mount_id as *mut i32 as usize;
    let flags = flags as usize;
    syscall5(
        SYS_NAME_TO_HANDLE_AT,
        dfd,
        filename_ptr,
        handle_ptr,
        mount_id_ptr,
        flags,
    )
    .map(drop)
}

/// High resolution sleep.
///
/// # Examples
///
/// ```
/// let t = nc::timespec_t {
///     tv_sec: 1,
///     tv_nsec: 0,
/// };
/// let ret = unsafe { nc::nanosleep(&t, None) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn nanosleep(req: &timespec_t, rem: Option<&mut timespec_t>) -> Result<(), Errno> {
    let req_ptr = req as *const timespec_t as usize;
    let rem_ptr = rem.map_or(core::ptr::null_mut::<timespec_t>() as usize, |rem| {
        rem as *mut timespec_t as usize
    });
    syscall2(SYS_NANOSLEEP, req_ptr, rem_ptr).map(drop)
}

/// Get file status.
///
/// # Examples
///
/// ```
/// let path = "/etc/passwd";
/// let mut stat = nc::stat_t::default();
/// let ret = unsafe { nc::newfstatat(nc::AT_FDCWD, path, &mut stat, nc::AT_SYMLINK_NOFOLLOW) };
/// assert!(ret.is_ok());
/// assert!(nc::S_ISREG(stat.st_mode as nc::mode_t));
/// ```
pub unsafe fn newfstatat<P: AsRef<Path>>(
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
    syscall4(SYS_NEWFSTATAT, dfd, filename_ptr, statbuf_ptr, flag).map(drop)
}

/// Open and possibly create a file within a directory.
///
/// # Examples
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

/// Open and possibly create a file (extended)
///
/// # Examples
///
/// ```
/// let path = "/etc/passwd";
/// let mut how = nc::open_how_t{
///   flags: nc::O_RDONLY as u64,
///   ..Default::default()
/// };
/// let ret = unsafe { nc::openat2(nc::AT_FDCWD, path, &mut how) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn openat2<P: AsRef<Path>>(
    dirfd: i32,
    pathname: P,
    how: &mut open_how_t,
) -> Result<i32, Errno> {
    let dirfd = dirfd as usize;
    let pathname = CString::new(pathname.as_ref());
    let pathname_ptr = pathname.as_ptr() as usize;
    let how_ptr = how as *mut open_how_t as usize;
    let how_size = core::mem::size_of::<open_how_t>();
    syscall4(SYS_OPENAT2, dirfd, pathname_ptr, how_ptr, how_size).map(|ret| ret as i32)
}

/// Obtain handle for an open file
pub unsafe fn open_by_handle_at(
    mount_fd: i32,
    handle: &mut file_handle_t,
    flags: i32,
) -> Result<i32, Errno> {
    let mount_fd = mount_fd as usize;
    let handle_ptr = handle as *mut file_handle_t as usize;
    let flags = flags as usize;
    syscall3(SYS_OPEN_BY_HANDLE_AT, mount_fd, handle_ptr, flags).map(|ret| ret as i32)
}

pub unsafe fn open_tree<P: AsRef<Path>>(dfd: i32, filename: P, flags: u32) -> Result<i32, Errno> {
    let dfd = dfd as usize;
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    let flags = flags as usize;
    syscall3(SYS_OPEN_TREE, dfd, filename_ptr, flags).map(|ret| ret as i32)
}

/// Set up performance monitoring.
pub unsafe fn perf_event_open(
    attr: &mut perf_event_attr_t,
    pid: pid_t,
    cpu: i32,
    group_fd: i32,
    flags: usize,
) -> Result<i32, Errno> {
    let attr_ptr = attr as *mut perf_event_attr_t as usize;
    let pid = pid as usize;
    let cpu = cpu as usize;
    let group_fd = group_fd as usize;
    syscall5(SYS_PERF_EVENT_OPEN, attr_ptr, pid, cpu, group_fd, flags).map(|ret| ret as i32)
}

/// Set the process execution domain.
///
/// Returns old execution domain.
pub unsafe fn personality(persona: u32) -> Result<u32, Errno> {
    let persona = persona as usize;
    syscall1(SYS_PERSONALITY, persona).map(|ret| ret as u32)
}

/// Obtain a duplicate of another process's file descriptor.
///
/// # Examples
///
/// ```
/// const STDOUT_FD: i32 = 1;
///
/// let pid = unsafe { nc::fork() };
///
/// assert!(pid.is_ok());
/// let pid = pid.unwrap();
/// if pid == 0 {
///     println!("In child process, pid: {}", unsafe { nc::getpid() });
///     let path = "/tmp/nc-pidfdopen";
///     let fd = unsafe {
///         nc::openat(
///             nc::AT_FDCWD,
///             path,
///             nc::O_CREAT | nc::O_WRONLY | nc::O_TRUNC,
///             0o644,
///         )
///     };
///     assert!(fd.is_ok());
///     let fd = fd.unwrap();
///     let ret = unsafe { nc::dup3(fd, STDOUT_FD, 0) };
///     assert!(ret.is_ok());
///     println!("[child] stdout redirected to file!");
///
///     let t = nc::timespec_t {
///         tv_sec: 2,
///         tv_nsec: 0,
///     };
///     unsafe {
///         let ret = nc::nanosleep(&t, None);
///         assert!(ret.is_ok());
///         let ret = nc::close(fd);
///         assert!(ret.is_ok());
///         let ret = nc::unlinkat(nc::AT_FDCWD, path, 0);
///         assert!(ret.is_ok());
///         nc::exit(0);
///     }
/// }
///
/// println!("[parent] child pid: {}", pid);
///
/// let t = nc::timespec_t {
///     tv_sec: 2,
///     tv_nsec: 0,
/// };
/// let ret = unsafe { nc::nanosleep(&t, None) };
/// assert!(ret.is_ok());
///
/// let pidfd = unsafe { nc::pidfd_open(pid, 0) };
/// if pidfd == Err(nc::errno::ENOSYS) {
///     eprintln!("PIDFD_OPEN syscall not supported in this system");
///     return;
/// }
/// let pidfd = pidfd.unwrap();
///
/// let child_stdout_fd = unsafe { nc::pidfd_getfd(pidfd, STDOUT_FD, 0) };
/// if child_stdout_fd == Err(nc::errno::ENOSYS) {
///     eprintln!("PIDFD_OPEN syscall not supported in this system");
///     return;
/// }
/// let child_stdout_fd = child_stdout_fd.unwrap();
/// let msg = b"Hello, msg from parent process\n";
/// let ret = unsafe { nc::write(child_stdout_fd, msg) };
/// assert!(ret.is_ok());
/// let nwrite = ret.unwrap();
/// assert_eq!(nwrite as usize, msg.len());
///
/// let ret = unsafe { nc::close(pidfd) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::close(child_stdout_fd) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn pidfd_getfd(pidfd: i32, target_fd: i32, flags: u32) -> Result<i32, Errno> {
    let pidfd = pidfd as usize;
    let target_fd = target_fd as usize;
    let flags = flags as usize;
    syscall3(SYS_PIDFD_GETFD, pidfd, target_fd, flags).map(|ret| ret as i32)
}

/// Obtain a file descriptor that refers to a process.
///
/// # Examples
///
/// ```
/// const STDOUT_FD: i32 = 1;
///
/// let pid = unsafe { nc::fork() };
/// assert!(pid.is_ok());
/// let pid = pid.unwrap();
///
/// if pid == 0 {
///     println!("In child process, pid: {}", unsafe { nc::getpid() });
///     let path = "/tmp/nc-pidfdopen";
///     let fd = unsafe {
///         nc::openat(
///             nc::AT_FDCWD,
///             path,
///             nc::O_CREAT | nc::O_WRONLY | nc::O_TRUNC,
///             0o644,
///         )
///     };
///     assert!(fd.is_ok());
///     let fd = fd.unwrap();
///     let ret = unsafe { nc::dup3(fd, STDOUT_FD, 0) };
///     assert!(ret.is_ok());
///
///     let t = nc::timespec_t {
///         tv_sec: 2,
///         tv_nsec: 0,
///     };
///     let ret = unsafe { nc::nanosleep(&t, None) };
///     assert!(ret.is_ok());
///     let ret = unsafe { nc::close(fd) };
///     assert!(ret.is_ok());
///     let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, path, 0) };
///     assert!(ret.is_ok());
///     unsafe { nc::exit(0) };
/// }
///
/// let t = nc::timespec_t {
///     tv_sec: 2,
///     tv_nsec: 0,
/// };
/// let ret = unsafe { nc::nanosleep(&t, None) };
/// assert!(ret.is_ok());
///
/// let pidfd = unsafe { nc::pidfd_open(pid, 0) };
/// if pidfd == Err(nc::errno::ENOSYS) {
///     eprintln!("PIDFD_OPEN syscall not supported in this system");
///     return;
/// }
/// let pidfd = pidfd.unwrap();
///
/// let child_stdout_fd = unsafe { nc::pidfd_getfd(pidfd, STDOUT_FD, 0) };
/// if child_stdout_fd == Err(nc::errno::ENOSYS) {
///     eprintln!("PIDFD_OPEN syscall not supported in this system");
///     return;
/// }
/// let child_stdout_fd = child_stdout_fd.unwrap();
/// let msg = b"Hello, msg from parent process\n";
/// let ret = unsafe { nc::write(child_stdout_fd, msg) };
/// assert!(ret.is_ok());
/// let nwrite = ret.unwrap();
/// assert_eq!(nwrite as usize, msg.len());
///
/// let ret = unsafe { nc::close(pidfd) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::close(child_stdout_fd) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn pidfd_open(pid: pid_t, flags: u32) -> Result<i32, Errno> {
    let pid = pid as usize;
    let flags = flags as usize;
    syscall2(SYS_PIDFD_OPEN, pid, flags).map(|ret| ret as i32)
}

/// Signal a process through a pidfd.
///
/// @pidfd:  file descriptor of the process
/// @sig:    signal to send
/// @info:   signal info
/// @flags:  future flags
///
/// The syscall currently only signals via `PIDTYPE_PID` which covers
/// `kill(<positive-pid>, <signal>)`. It does not signal threads or process
/// groups.
/// In order to extend the syscall to threads and process groups the @flags
/// argument should be used. In essence, the @flags argument will determine
/// what is signaled and not the file descriptor itself. Put in other words,
/// grouping is a property of the flags argument not a property of the file
/// descriptor.
///
/// # Examples
///
/// ```
/// const STDOUT_FD: i32 = 1;
///
/// let pid = unsafe { nc::fork() };
///
/// assert!(pid.is_ok());
/// let pid = pid.unwrap();
/// if pid == 0 {
///     let curr_pid = unsafe { nc::getpid() };
///     println!("In child process, pid: {}", curr_pid);
///     let path = "/tmp/nc-pidfdopen";
///     let fd = unsafe {
///         nc::openat(
///             nc::AT_FDCWD,
///             path,
///             nc::O_CREAT | nc::O_WRONLY | nc::O_TRUNC,
///             0o644,
///         )
///     };
///     assert!(fd.is_ok());
///     let fd = fd.unwrap();
///     let ret = unsafe { nc::dup3(fd, STDOUT_FD, 0) };
///     assert!(ret.is_ok());
///     println!("[child] stdout redirected to file!");
///
///     let t = nc::timespec_t {
///         tv_sec: 2,
///         tv_nsec: 0,
///     };
///     unsafe {
///         let ret = nc::nanosleep(&t, None);
///         assert!(ret.is_ok());
///         let ret = nc::close(fd);
///         assert!(ret.is_ok());
///         let ret = nc::unlinkat(nc::AT_FDCWD, path, 0);
///         assert!(ret.is_ok());
///         nc::exit(0);
///     }
/// }
///
/// println!("[parent] child pid: {}", pid);
///
/// let t = nc::timespec_t {
///     tv_sec: 2,
///     tv_nsec: 0,
/// };
/// let ret = unsafe { nc::nanosleep(&t, None) };
/// assert!(ret.is_ok());
///
/// let pidfd = unsafe { nc::pidfd_open(pid, 0) };
/// assert!(pidfd.is_ok());
/// let pidfd = pidfd.unwrap();
///
/// let ret = unsafe { nc::pidfd_getfd(pidfd, STDOUT_FD, 0) };
/// println!("ret: {:?}", ret);
/// if let Err(errno) = ret {
///     eprintln!("pidfd_getfd() failed, err: {}", nc::strerror(errno));
/// }
/// let child_stdout_fd = ret.unwrap();
/// let msg = b"Hello, msg from parent process\n";
/// let ret = unsafe { nc::write(child_stdout_fd, msg) };
/// assert!(ret.is_ok());
/// let nwrite = ret.unwrap();
/// assert_eq!(nwrite as usize, msg.len());
///
/// let ret = unsafe { nc::pidfd_send_signal(pidfd, nc::SIGKILL, None, 0) };
/// println!("ret: {:?}", ret);
/// if let Err(errno) = ret {
///     eprintln!("pidfd_send_signal() failed, err: {}", nc::strerror(errno));
/// }
///
/// unsafe {
///     let ret = nc::close(pidfd);
///     assert!(ret.is_ok());
///     let ret = nc::close(child_stdout_fd);
///     assert!(ret.is_ok());
/// }
/// ```
pub unsafe fn pidfd_send_signal(
    pidfd: i32,
    sig: i32,
    info: Option<&mut siginfo_t>,
    flags: u32,
) -> Result<(), Errno> {
    let pidfd = pidfd as usize;
    let sig = sig as usize;
    let info_ptr = info.map_or(core::ptr::null_mut::<siginfo_t>() as usize, |info| {
        info as *mut siginfo_t as usize
    });
    let flags = flags as usize;
    syscall4(SYS_PIDFD_SEND_SIGNAL, pidfd, sig, info_ptr, flags).map(drop)
}

/// Create a pipe.
///
/// # Examples
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

/// Change the root filesystem.
pub unsafe fn pivot_root<P: AsRef<Path>>(new_root: P, put_old: P) -> Result<(), Errno> {
    let new_root = CString::new(new_root.as_ref());
    let new_root_ptr = new_root.as_ptr() as usize;
    let put_old = CString::new(put_old.as_ref());
    let put_old_ptr = put_old.as_ptr() as usize;
    syscall2(SYS_PIVOT_ROOT, new_root_ptr, put_old_ptr).map(drop)
}

/// Create a new protection key.
pub unsafe fn pkey_alloc(flags: usize, init_val: u32) -> Result<i32, Errno> {
    let init_val = init_val as usize;
    syscall2(SYS_PKEY_ALLOC, flags, init_val).map(|ret| ret as i32)
}

/// Free a protection key.
pub unsafe fn pkey_free(pkey: i32) -> Result<(), Errno> {
    let pkey = pkey as usize;
    syscall1(SYS_PKEY_FREE, pkey).map(drop)
}

/// Set protection on a region of memory.
pub unsafe fn pkey_mprotect(
    start: *const core::ffi::c_void,
    len: size_t,
    prot: i32,
    pkey: i32,
) -> Result<(), Errno> {
    let start = start as usize;
    let prot = prot as usize;
    let pkey = pkey as usize;
    syscall4(SYS_PKEY_MPROTECT, start, len, prot, pkey).map(drop)
}

/// Wait for some event on a file descriptor.
pub unsafe fn ppoll(
    fds: &mut [pollfd_t],
    timeout: Option<&timespec_t>,
    sig_mask: Option<&sigset_t>,
) -> Result<i32, Errno> {
    use core::ptr::null;

    let fds_ptr = fds.as_mut_ptr() as usize;
    let nfds = fds.len();
    let timeout_ptr = timeout.map_or(null::<timespec_t>() as usize, |timeout| {
        timeout as *const timespec_t as usize
    });
    let sig_mask_ptr = sig_mask.map_or(null::<sigset_t>() as usize, |sig_mask| {
        sig_mask as *const sigset_t as usize
    });
    let sig_set_size = core::mem::size_of::<sigset_t>();
    syscall5(
        SYS_PPOLL,
        fds_ptr,
        nfds,
        timeout_ptr,
        sig_mask_ptr,
        sig_set_size,
    )
    .map(|ret| ret as i32)
}

/// Operations on a process.
pub unsafe fn prctl(
    option: i32,
    arg2: usize,
    arg3: usize,
    arg4: usize,
    arg5: usize,
) -> Result<i32, Errno> {
    let option = option as usize;
    syscall5(SYS_PRCTL, option, arg2, arg3, arg4, arg5).map(|ret| ret as i32)
}

/// Read from a file descriptor without changing file offset.
///
/// # Examples
///
/// ```
/// let path = "/etc/passwd";
/// let ret = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_RDONLY, 0) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
/// let mut buf = [0_u8; 128];
/// let read_count = 64;
/// let ret = unsafe { nc::pread64(fd, &mut buf[..read_count], 0) };
/// assert!(ret.is_ok());
/// assert_eq!(ret, Ok(read_count as nc::ssize_t));
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn pread64(fd: i32, buf: &mut [u8], offset: off_t) -> Result<ssize_t, Errno> {
    let fd = fd as usize;
    let count = buf.len();
    let buf_ptr = buf.as_mut_ptr() as usize;
    let offset = offset as usize;
    syscall4(SYS_PREAD64, fd, buf_ptr, count, offset).map(|ret| ret as ssize_t)
}

/// Read from a file descriptor without changing file offset.
///
/// # Examples
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
/// let ret = unsafe { nc::preadv(fd as usize, &mut iov, 0, iov_len - 1) };
/// assert!(ret.is_ok());
/// assert_eq!(ret, Ok(capacity as nc::ssize_t));
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn preadv(
    fd: usize,
    vec: &mut [iovec_t],
    pos_l: usize,
    pos_h: usize,
) -> Result<ssize_t, Errno> {
    let vec_ptr = vec.as_mut_ptr() as usize;
    let vec_len = vec.len();
    syscall5(SYS_PREADV, fd, vec_ptr, vec_len, pos_l, pos_h).map(|ret| ret as ssize_t)
}

/// Read from a file descriptor without changing file offset.
///
/// # Examples
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
/// let flags = 0;
/// let ret = unsafe { nc::preadv2(fd as usize, &mut iov, 0, iov_len - 1, flags) };
/// assert!(ret.is_ok());
/// assert_eq!(ret, Ok(capacity as nc::ssize_t));
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn preadv2(
    fd: usize,
    vec: &mut [iovec_t],
    pos_l: usize,
    pos_h: usize,
    flags: rwf_t,
) -> Result<ssize_t, Errno> {
    let vec_ptr = vec.as_mut_ptr() as usize;
    let vec_len = vec.len();
    let flags = flags as usize;
    syscall6(SYS_PREADV2, fd, vec_ptr, vec_len, pos_l, pos_h, flags).map(|ret| ret as ssize_t)
}

/// Get/set the resource limits of an arbitary process.
///
/// # Examples
///
/// ```
/// let mut old_limit = nc::rlimit64_t::default();
/// let ret = unsafe { nc::prlimit64(nc::getpid(), nc::RLIMIT_NOFILE, None, Some(&mut old_limit)) };
/// assert!(ret.is_ok());
/// assert!(old_limit.rlim_cur > 0);
/// assert!(old_limit.rlim_max > 0);
/// ```
pub unsafe fn prlimit64(
    pid: pid_t,
    resource: i32,
    new_limit: Option<&rlimit64_t>,
    old_limit: Option<&mut rlimit64_t>,
) -> Result<(), Errno> {
    let pid = pid as usize;
    let resource = resource as usize;
    let new_limit_ptr = new_limit.map_or(core::ptr::null::<rlimit64_t>() as usize, |new_limit| {
        new_limit as *const rlimit64_t as usize
    });
    let old_limit_ptr = old_limit
        .map_or(core::ptr::null_mut::<rlimit64_t>() as usize, |old_limit| {
            old_limit as *mut rlimit64_t as usize
        });
    syscall4(SYS_PRLIMIT64, pid, resource, new_limit_ptr, old_limit_ptr).map(drop)
}

/// Transfer data between process address spaces
pub unsafe fn process_vm_readv(
    pid: pid_t,
    lvec: &[iovec_t],
    rvec: &[iovec_t],
    flags: usize,
) -> Result<ssize_t, Errno> {
    let pid = pid as usize;
    let lvec_ptr = lvec.as_ptr() as usize;
    let lvec_len = lvec.len();
    let rvec_ptr = rvec.as_ptr() as usize;
    let rvec_len = rvec.len();
    syscall6(
        SYS_PROCESS_VM_READV,
        pid,
        lvec_ptr,
        lvec_len,
        rvec_ptr,
        rvec_len,
        flags,
    )
    .map(|ret| ret as ssize_t)
}

/// Transfer data between process address spaces
pub unsafe fn process_vm_writev(
    pid: pid_t,
    lvec: &[iovec_t],
    rvec: &[iovec_t],
    flags: usize,
) -> Result<ssize_t, Errno> {
    let pid = pid as usize;
    let lvec_ptr = lvec.as_ptr() as usize;
    let lvec_len = lvec.len();
    let rvec_ptr = rvec.as_ptr() as usize;
    let rvec_len = rvec.len();
    syscall6(
        SYS_PROCESS_VM_WRITEV,
        pid,
        lvec_ptr,
        lvec_len,
        rvec_ptr,
        rvec_len,
        flags,
    )
    .map(|ret| ret as ssize_t)
}

/// Sychronous I/O multiplexing.
///
/// Most architectures can't handle 7-argument syscalls. So we provide a
/// 6-argument version where the sixth argument is a pointer to a structure
/// which has a pointer to the `sigset_t` itself followed by a `size_t` containing
/// the sigset size.
pub unsafe fn pselect6(
    nfds: i32,
    read_fds: Option<&mut fd_set_t>,
    write_fds: Option<&mut fd_set_t>,
    except_fds: Option<&mut fd_set_t>,
    timeout: Option<&timespec_t>,
    sigmask: Option<&sigset_t>,
) -> Result<i32, Errno> {
    use core::ptr::{null, null_mut};
    let nfds = nfds as usize;
    let read_fds_ptr = read_fds.map_or(null_mut::<fd_set_t>() as usize, |read_fds| {
        read_fds as *mut fd_set_t as usize
    });
    let write_fds_ptr = write_fds.map_or(null_mut::<fd_set_t>() as usize, |write_fds| {
        write_fds as *mut fd_set_t as usize
    });
    let except_fds_ptr = except_fds.map_or(null_mut::<fd_set_t>() as usize, |except_fds| {
        except_fds as *mut fd_set_t as usize
    });
    let timeout_ptr = timeout.map_or(null::<timespec_t>() as usize, |timeout| {
        timeout as *const timespec_t as usize
    });
    let sigmask_ptr = sigmask.map_or(null::<sigset_t>() as usize, |sigmask| {
        sigmask as *const sigset_t as usize
    });
    syscall6(
        SYS_PSELECT6,
        nfds,
        read_fds_ptr,
        write_fds_ptr,
        except_fds_ptr,
        timeout_ptr,
        sigmask_ptr,
    )
    .map(|ret| ret as i32)
}

/// Process trace.
pub unsafe fn ptrace(request: i32, pid: pid_t, addr: usize, data: usize) -> Result<isize, Errno> {
    let request = request as usize;
    let pid = pid as usize;
    syscall4(SYS_PTRACE, request, pid, addr, data).map(|ret| ret as isize)
}

/// Write to a file descriptor without changing file offset.
///
/// # Examples
///
/// ```
/// let path = "/tmp/nc-pwrite64";
/// let ret = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_WRONLY | nc::O_CREAT, 0o644) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
/// let msg = "Hello, Rust";
/// let ret = unsafe { nc::pwrite64(fd, msg.as_bytes(), 0) };
/// assert!(ret.is_ok());
/// assert_eq!(ret, Ok(msg.len() as nc::ssize_t));
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, path, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn pwrite64(fd: i32, buf: &[u8], offset: loff_t) -> Result<ssize_t, Errno> {
    let fd = fd as usize;
    let count = buf.len();
    let buf_ptr = buf.as_ptr() as usize;
    let offset = offset as usize;
    syscall4(SYS_PWRITE64, fd, buf_ptr, count, offset).map(|ret| ret as ssize_t)
}

/// Write to a file descriptor without changing file offset.
///
/// # Examples
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
/// let ret = unsafe { nc::readv(fd as usize, &mut iov) };
/// assert!(ret.is_ok());
/// assert_eq!(ret, Ok(capacity as nc::ssize_t));
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
///
/// let path_out = "/tmp/nc-pwritev";
/// let ret = unsafe { nc::openat(nc::AT_FDCWD, path_out, nc::O_WRONLY | nc::O_CREAT, 0o644) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
/// let ret = unsafe { nc::pwritev(fd as usize, &iov, 0, iov.len() - 1) };
/// assert!(ret.is_ok());
/// assert_eq!(ret, Ok(capacity as nc::ssize_t));
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, path_out, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn pwritev(
    fd: usize,
    vec: &[iovec_t],
    pos_l: usize,
    pos_h: usize,
) -> Result<ssize_t, Errno> {
    let vec_ptr = vec.as_ptr() as usize;
    let vec_len = vec.len();
    syscall5(SYS_PWRITEV, fd, vec_ptr, vec_len, pos_l, pos_h).map(|ret| ret as ssize_t)
}

/// Write to a file descriptor without changing file offset.
///
/// # Examples
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
/// let ret = unsafe { nc::readv(fd as usize, &mut iov) };
/// assert!(ret.is_ok());
/// assert_eq!(ret, Ok(capacity as nc::ssize_t));
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
///
/// let path_out = "/tmp/nc-pwritev2";
/// let ret = unsafe { nc::openat(nc::AT_FDCWD, path_out, nc::O_WRONLY | nc::O_CREAT, 0o644) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
/// let flags = nc::RWF_DSYNC | nc::RWF_APPEND;
/// let ret = unsafe { nc::pwritev2(fd as usize, &iov, 0, iov.len() - 1, flags) };
/// assert!(ret.is_ok());
/// assert_eq!(ret, Ok(capacity as nc::ssize_t));
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, path_out, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn pwritev2(
    fd: usize,
    vec: &[iovec_t],
    pos_l: usize,
    pos_h: usize,
    flags: rwf_t,
) -> Result<ssize_t, Errno> {
    let vec_ptr = vec.as_ptr() as usize;
    let vec_len = vec.len();
    let flags = flags as usize;
    syscall6(SYS_PWRITEV2, fd, vec_ptr, vec_len, pos_l, pos_h, flags).map(|ret| ret as ssize_t)
}

/// Manipulate disk quotes.
pub unsafe fn quotactl<P: AsRef<Path>>(
    cmd: u32,
    special: P,
    id: qid_t,
    addr: *const core::ffi::c_void,
) -> Result<(), Errno> {
    let cmd = cmd as usize;
    let special = CString::new(special.as_ref());
    let special_ptr = special.as_ptr() as usize;
    let id = id as usize;
    let addr = addr as usize;
    syscall4(SYS_QUOTACTL, cmd, special_ptr, id, addr).map(drop)
}

/// Manipulate disk quotes.
pub unsafe fn quotactl_fd<P: AsRef<Path>>(
    fd: i32,
    cmd: u32,
    id: qid_t,
    addr: *const core::ffi::c_void,
) -> Result<(), Errno> {
    let fd = fd as usize;
    let cmd = cmd as usize;
    let id = id as usize;
    let addr = addr as usize;
    syscall4(SYS_QUOTACTL_FD, fd, cmd, id, addr).map(drop)
}

/// Read from a file descriptor.
///
/// # Examples
///
/// ```
/// let path = "/etc/passwd";
/// let ret = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_RDONLY, 0) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
/// let mut buf = [0_u8; 4 * 1024];
/// let ret = unsafe { nc::read(fd, &mut buf) };
/// assert!(ret.is_ok());
/// let n_read = ret.unwrap();
/// assert!(n_read <= buf.len() as nc::ssize_t);
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn read(fd: i32, buf: &mut [u8]) -> Result<ssize_t, Errno> {
    let fd = fd as usize;
    let buf_ptr = buf.as_mut_ptr() as usize;
    let count = buf.len();
    syscall3(SYS_READ, fd, buf_ptr, count).map(|ret| ret as ssize_t)
}

/// Initialize file head into page cache.
///
/// # Examples
///
/// ```
/// let path = "/etc/passwd";
/// let ret = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_RDONLY, 0) };
/// let fd = ret.unwrap();
/// let ret = unsafe { nc::readahead(fd, 0, 64) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn readahead(fd: i32, offset: off_t, count: size_t) -> Result<(), Errno> {
    let fd = fd as usize;
    let offset = offset as usize;
    syscall3(SYS_READAHEAD, fd, offset, count).map(drop)
}

/// Read value of a symbolic link.
///
/// # Examples
///
/// ```
/// let oldname = "/etc/passwd";
/// let newname = "/tmp/nc-readlinkat";
/// let ret = unsafe { nc::symlinkat(oldname, nc::AT_FDCWD, newname) };
/// assert!(ret.is_ok());
/// let mut buf = [0_u8; nc::PATH_MAX as usize + 1];
/// let buf_len = buf.len();
/// let ret = unsafe { nc::readlinkat(nc::AT_FDCWD, newname, &mut buf) };
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
) -> Result<ssize_t, Errno> {
    let dirfd = dirfd as usize;
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    let buf_ptr = buf.as_mut_ptr() as usize;
    let buf_len = buf.len();
    syscall4(SYS_READLINKAT, dirfd, filename_ptr, buf_ptr, buf_len).map(|ret| ret as ssize_t)
}

/// Read from a file descriptor into multiple buffers.
///
/// # Examples
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
///         iov_base: item.as_ptr() as *const c_void,
///         iov_len: item.len(),
///     });
/// }
/// let ret = unsafe { nc::readv(fd as usize, &mut iov) };
/// assert!(ret.is_ok());
/// assert_eq!(ret, Ok(capacity as nc::ssize_t));
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn readv(fd: usize, iov: &mut [iovec_t]) -> Result<ssize_t, Errno> {
    let iov_ptr = iov.as_mut_ptr() as usize;
    let len = iov.len();
    syscall3(SYS_READV, fd, iov_ptr, len).map(|ret| ret as ssize_t)
}

/// Reboot or enable/disable Ctrl-Alt-Del.
///
/// # Examples
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
///
/// # Examples
///
/// ```
/// use std::thread;
///
/// const READ_SIDE: usize = 0;
/// const WRITE_SIDE: usize = 1;
///
/// fn main() {
///     let mut fds = [-1_i32; 2];
///
///     let ret = unsafe { nc::socketpair(nc::AF_UNIX, nc::SOCK_STREAM, 0, &mut fds) };
///     assert!(ret.is_ok());
///     println!("socket pairs: {}, {}", fds[0], fds[1]);
///
///     // Start worker thread
///     thread::spawn(move || {
///         println!("worker thread started");
///         let msg = "Hello, Rust";
///         println!("[worker] Will send msg: {msg}");
///         let ret = unsafe { nc::sendto(fds[WRITE_SIDE], msg.as_bytes(), 0, None, 0) };
///         assert!(ret.is_ok());
///         assert_eq!(ret, Ok(msg.len() as nc::ssize_t));
///     });
///
///     let mut buf = [0_u8; 1024];
///     let ret = unsafe { nc::recvfrom(fds[READ_SIDE], &mut buf, 0, None, None) };
///     assert!(ret.is_ok());
///     let nread = ret.unwrap();
///     let msg = std::str::from_utf8(&buf[..nread as usize]).unwrap();
///     println!("[main] recv msg: {msg}");
///
///     unsafe {
///         let _ = nc::close(fds[0]);
///         let _ = nc::close(fds[1]);
///     }
/// }
/// ```
pub unsafe fn recvfrom(
    sockfd: i32,
    buf: &mut [u8],
    flags: i32,
    src_addr: Option<*mut sockaddr_t>,
    addrlen: Option<&mut socklen_t>,
) -> Result<ssize_t, Errno> {
    let sockfd = sockfd as usize;
    let buf_ptr = buf.as_mut_ptr() as usize;
    let buflen = buf.len();
    let flags = flags as usize;
    let src_addr_ptr = src_addr.map_or(core::ptr::null_mut::<sockaddr_t>() as usize, |src_addr| {
        src_addr as usize
    });
    let addrlen_ptr = addrlen.map_or(core::ptr::null_mut::<socklen_t>() as usize, |addrlen| {
        addrlen as *mut socklen_t as usize
    });
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
///
/// Returns number of messages received.
///
/// # Examples
/// ```
/// use std::ffi::c_void;
/// use std::ptr;
/// use std::thread;
///
/// const READ_SIDE: usize = 0;
/// const WRITE_SIDE: usize = 1;
///
/// fn main() {
///     let mut fds = [-1_i32; 2];
///
///     let ret = unsafe { nc::socketpair(nc::AF_UNIX, nc::SOCK_STREAM, 0, &mut fds) };
///     assert!(ret.is_ok());
///     println!("socket pairs: {}, {}", fds[0], fds[1]);
///
///     // Start worker thread
///     thread::spawn(move || {
///         println!("worker thread started");
///         let msg = "Hello, Rust";
///         println!("[worker] Will send msg: {msg}");
///         let mut iov = [nc::iovec_t {
///             iov_base: msg.as_ptr() as *const c_void,
///             iov_len: msg.len(),
///         }];
///         let msg_hdr = nc::msghdr_t {
///             msg_name: ptr::null(),
///             msg_namelen: 0,
///             msg_iov: iov.as_mut_ptr(),
///             msg_iovlen: iov.len(),
///             msg_control: ptr::null(),
///             msg_controllen: 0,
///             msg_flags: 0,
///         };
///         let mmsg_hdr = [nc::mmsghdr_t {
///             msg_hdr,
///             msg_len: 0,
///         }];
///         let ret = unsafe { nc::sendmmsg(fds[WRITE_SIDE], &mmsg_hdr, 0) };
///         assert!(ret.is_ok());
///         //assert_eq!(ret, Ok(msg.len() as nc::ssize_t));
///     });
///
///     let mut buf = [[0_u8; 64]; 4];
///     let mut iov = Vec::with_capacity(buf.len());
///     for ref mut item in (&mut buf).iter() {
///         iov.push(nc::iovec_t {
///             iov_base: item.as_ptr() as *const c_void,
///             iov_len: item.len(),
///         });
///     }
///     let msg_hdr = nc::msghdr_t {
///         msg_name: ptr::null(),
///         msg_namelen: 0,
///         msg_iov: iov.as_mut_ptr(),
///         msg_iovlen: iov.len(),
///         msg_control: ptr::null(),
///         msg_controllen: 0,
///         msg_flags: 0,
///     };
///     let mut mmsg_hdr = [nc::mmsghdr_t {
///         msg_hdr,
///         msg_len: 0,
///     }];
///     let ret = unsafe { nc::recvmmsg(fds[READ_SIDE], &mut mmsg_hdr, 0, None) };
///     assert!(ret.is_ok());
///     let msgs_read = ret.unwrap();
///     assert_eq!(msgs_read, 1);
///     // We only read the first buffer block.
///     for i in 0..msgs_read {
///         let nread = mmsg_hdr[i].msg_len;
///         let msg = std::str::from_utf8(&buf[0][..nread as usize]).unwrap();
///         println!("[main] recv msg: {msg}");
///     }
///
///     unsafe {
///         let _ = nc::close(fds[0]);
///         let _ = nc::close(fds[1]);
///     }
/// }
/// ```
pub unsafe fn recvmmsg(
    sockfd: i32,
    msgvec: &mut [mmsghdr_t],
    flags: i32,
    timeout: Option<&mut timespec_t>,
) -> Result<size_t, Errno> {
    let sockfd = sockfd as usize;
    let msgvec_ptr = msgvec.as_mut_ptr() as usize;
    let vlen = msgvec.len();
    let flags = flags as usize;
    let timeout_ptr = timeout.map_or(core::ptr::null_mut::<timespec_t>() as usize, |timeout| {
        timeout as *mut timespec_t as usize
    });
    syscall5(SYS_RECVMMSG, sockfd, msgvec_ptr, vlen, flags, timeout_ptr)
}

/// Receive a msg from a socket.
///
/// # Examples
///
/// ```
/// use std::ffi::c_void;
/// use std::ptr;
/// use std::thread;
///
/// const READ_SIDE: usize = 0;
/// const WRITE_SIDE: usize = 1;
///
/// fn main() {
///     let mut fds = [-1_i32; 2];
///
///     let ret = unsafe { nc::socketpair(nc::AF_UNIX, nc::SOCK_STREAM, 0, &mut fds) };
///     assert!(ret.is_ok());
///     println!("socket pairs: {}, {}", fds[0], fds[1]);
///
///     // Start worker thread
///     thread::spawn(move || {
///         println!("worker thread started");
///         let msg = "Hello, Rust";
///         println!("[worker] Will send msg: {msg}");
///         let mut iov = [nc::iovec_t {
///             iov_base: msg.as_ptr() as *const c_void,
///             iov_len: msg.len(),
///         }];
///         let msg_hdr = nc::msghdr_t {
///             msg_name: ptr::null(),
///             msg_namelen: 0,
///             msg_iov: iov.as_mut_ptr(),
///             msg_iovlen: iov.len(),
///             msg_control: ptr::null(),
///             msg_controllen: 0,
///             msg_flags: 0,
///         };
///         let ret = unsafe { nc::sendmsg(fds[WRITE_SIDE], &msg_hdr, 0) };
///         assert!(ret.is_ok());
///         assert_eq!(ret, Ok(msg.len() as nc::ssize_t));
///     });
///
///     let mut buf = [[0_u8; 64]; 4];
///     let mut iov = Vec::with_capacity(buf.len());
///     for ref mut item in (&mut buf).iter() {
///         iov.push(nc::iovec_t {
///             iov_base: item.as_ptr() as *const c_void,
///             iov_len: item.len(),
///         });
///     }
///     let mut msg_hdr = nc::msghdr_t {
///         msg_name: ptr::null(),
///         msg_namelen: 0,
///         msg_iov: iov.as_mut_ptr(),
///         msg_iovlen: iov.len(),
///         msg_control: ptr::null(),
///         msg_controllen: 0,
///         msg_flags: 0,
///     };
///     let ret = unsafe { nc::recvmsg(fds[READ_SIDE], &mut msg_hdr, 0) };
///     assert!(ret.is_ok());
///     let nread = ret.unwrap();
///     // We only read the first buffer block.
///     let msg = std::str::from_utf8(&buf[0][..nread as usize]).unwrap();
///     println!("[main] recv msg: {msg}");
///
///     unsafe {
///         let _ = nc::close(fds[0]);
///         let _ = nc::close(fds[1]);
///     }
/// }
/// ```
pub unsafe fn recvmsg(sockfd: i32, msg: &mut msghdr_t, flags: i32) -> Result<ssize_t, Errno> {
    let sockfd = sockfd as usize;
    let msg_ptr = msg as *mut msghdr_t as usize;
    let flags = flags as usize;
    syscall3(SYS_RECVMSG, sockfd, msg_ptr, flags).map(|ret| ret as ssize_t)
}

/// Create a nonlinear file mapping.
///
/// Deprecated.
pub unsafe fn remap_file_pages(
    start: *const core::ffi::c_void,
    size: size_t,
    prot: i32,
    pgoff: off_t,
    flags: i32,
) -> Result<(), Errno> {
    let start = start as usize;
    let prot = prot as usize;
    let pgoff = pgoff as usize;
    // NOTE(Shaohua): Type of flags is usize in kernel.
    let flags = flags as usize;
    syscall5(SYS_REMAP_FILE_PAGES, start, size, prot, pgoff, flags).map(drop)
}

/// Remove an extended attribute.
///
/// # Examples
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
///         attr_value.as_bytes(),
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
/// # Examples
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

/// Change name or location of a file.
///
/// # Examples
///
/// ```
/// let path = "/tmp/nc-renameat2";
/// let ret = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_WRONLY | nc::O_CREAT, 0o644) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// let new_path = "/tmp/nc-renameat2-new";
/// let flags = nc::RENAME_NOREPLACE;
/// let ret = unsafe { nc::renameat2(nc::AT_FDCWD, path, nc::AT_FDCWD, new_path, flags) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, new_path, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn renameat2<P: AsRef<Path>>(
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
        SYS_RENAMEAT2,
        olddfd,
        oldfilename_ptr,
        newdfd,
        newfilename_ptr,
        flags,
    )
    .map(drop)
}

/// Request a key from kernel's key management facility.
pub unsafe fn request_key<P: AsRef<Path>>(
    type_: P,
    description: P,
    callout_info: Option<P>,
    dest_keyring: key_serial_t,
) -> Result<key_serial_t, Errno> {
    let type_ = CString::new(type_.as_ref());
    let type_ptr = type_.as_ptr() as usize;
    let description = CString::new(description.as_ref());
    let description_ptr = description.as_ptr() as usize;
    let callout_info = callout_info.map(|callout_info| CString::new(callout_info.as_ref()));
    let callout_info_ptr = callout_info.map_or(core::ptr::null::<u8>() as usize, |callout_info| {
        callout_info.as_ptr() as usize
    });
    let dest_keyring = dest_keyring as usize;
    syscall4(
        SYS_REQUEST_KEY,
        type_ptr,
        description_ptr,
        callout_info_ptr,
        dest_keyring,
    )
    .map(|ret| ret as key_serial_t)
}

/// Restart a system call after interruption by a stop signal.
pub unsafe fn restart_syscall() -> Result<i32, Errno> {
    syscall0(SYS_RESTART_SYSCALL).map(|ret| ret as i32)
}

/// Setup restartable sequences for caller thread.
pub unsafe fn rseq(rseq: &mut [rseq_t], flags: i32, sig: u32) -> Result<i32, Errno> {
    let rseq_ptr = rseq.as_mut_ptr() as usize;
    let rseq_len = rseq.len();
    let flags = flags as usize;
    let sig = sig as usize;
    syscall4(SYS_RSEQ, rseq_ptr, rseq_len, flags, sig).map(|ret| ret as i32)
}

/// Examine and change a signal action.
///
/// # Examples
///
/// ```
/// fn handle_sigterm(sig: i32) {
///     assert_eq!(sig, nc::SIGTERM);
/// }
///
/// let sa = nc::new_sigaction(handle_sigterm);
/// let ret = unsafe { nc::rt_sigaction(nc::SIGTERM, Some(&sa), None) };
/// let ret = unsafe { nc::kill(nc::getpid(), nc::SIGTERM) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn rt_sigaction(
    sig: i32,
    act: Option<&sigaction_t>,
    old_act: Option<&mut sigaction_t>,
) -> Result<(), Errno> {
    let sig = sig as usize;
    let act_ptr = act.map_or(core::ptr::null::<sigaction_t>() as usize, |act| {
        act as *const sigaction_t as usize
    });
    let old_act_ptr = old_act.map_or(core::ptr::null_mut::<sigaction_t>() as usize, |old_act| {
        old_act as *mut sigaction_t as usize
    });
    let sigset_size = core::mem::size_of::<sigset_t>();
    syscall4(SYS_RT_SIGACTION, sig, act_ptr, old_act_ptr, sigset_size).map(drop)
}

/// Examine pending signals.
pub unsafe fn rt_sigpending(set: &mut sigset_t) -> Result<(), Errno> {
    let set_ptr = set as *mut sigset_t as usize;
    let sig_set_size = core::mem::size_of::<sigset_t>();
    syscall2(SYS_RT_SIGPENDING, set_ptr, sig_set_size).map(drop)
}

/// Change the list of currently blocked signals.
pub unsafe fn rt_sigprocmask(
    how: i32,
    set: Option<&sigset_t>,
    oldset: Option<&mut sigset_t>,
) -> Result<(), Errno> {
    let how = how as usize;
    let set_ptr = set.map_or(core::ptr::null::<sigset_t>() as usize, |set| {
        set as *const sigset_t as usize
    });
    let oldset_ptr = oldset.map_or(core::ptr::null_mut::<sigset_t>() as usize, |oldset| {
        oldset as *mut sigset_t as usize
    });
    let sig_set_size = core::mem::size_of::<sigset_t>();
    syscall4(SYS_RT_SIGPROCMASK, how, set_ptr, oldset_ptr, sig_set_size).map(drop)
}

/// Queue a signal and data.
///
/// Send signal information to a thread.
pub unsafe fn rt_sigqueueinfo(pid: pid_t, sig: i32, info: &mut siginfo_t) -> Result<(), Errno> {
    let pid = pid as usize;
    let sig = sig as usize;
    let info_ptr = info as *mut siginfo_t as usize;
    syscall3(SYS_RT_SIGQUEUEINFO, pid, sig, info_ptr).map(drop)
}

/// Return from signal handler and cleanup stack frame.
///
/// Never returns.
pub unsafe fn rt_sigreturn() {
    let _ret = syscall0(SYS_RT_SIGRETURN);
}

/// Wait for a signal.
///
/// Always returns `Errno`, normally `EINTR`.
///
/// # Examples
///
/// ```
/// let pid = unsafe { nc::fork() };
/// assert!(pid.is_ok());
/// let pid = pid.unwrap();
/// assert!(pid >= 0);
///
/// if pid == 0 {
///     // child process.
///     let mask = nc::sigset_t::default();
///     let ret = unsafe { nc::rt_sigsuspend(&mask) };
///     assert!(ret.is_ok());
/// } else {
///     // parent process.
///     let t = nc::timespec_t {
///         tv_sec: 1,
///         tv_nsec: 0,
///     };
///     let ret = unsafe { nc::nanosleep(&t, None) };
///     assert!(ret.is_ok());
///
///     let ret = unsafe { nc::kill(pid, nc::SIGTERM) };
///     assert!(ret.is_ok());
/// }
/// ```
pub unsafe fn rt_sigsuspend(set: &sigset_t) -> Result<(), Errno> {
    let set_ptr = set as *const sigset_t as usize;
    let sigset_size = core::mem::size_of::<sigset_t>();
    syscall2(SYS_RT_SIGSUSPEND, set_ptr, sigset_size).map(drop)
}

/// Synchronously wait for queued signals.
pub unsafe fn rt_sigtimedwait(
    set: &sigset_t,
    info: Option<&mut siginfo_t>,
    ts: &timespec_t,
) -> Result<i32, Errno> {
    let set_ptr = set as *const sigset_t as usize;
    let info_ptr = info.map_or(core::ptr::null_mut::<siginfo_t>() as usize, |info| {
        info as *mut siginfo_t as usize
    });
    let ts_ptr = ts as *const timespec_t as usize;
    let sig_set_size = core::mem::size_of::<sigset_t>();
    syscall4(SYS_RT_SIGTIMEDWAIT, set_ptr, info_ptr, ts_ptr, sig_set_size).map(|ret| ret as i32)
}

/// Queue a signal and data.
pub unsafe fn rt_tgsigqueueinfo(
    tgid: pid_t,
    tid: pid_t,
    sig: i32,
    info: &mut siginfo_t,
) -> Result<(), Errno> {
    let tgid = tgid as usize;
    let tid = tid as usize;
    let sig = sig as usize;
    let info_ptr = info as *mut siginfo_t as usize;
    syscall4(SYS_RT_TGSIGQUEUEINFO, tgid, tid, sig, info_ptr).map(drop)
}

/// Get a thread's CPU affinity mask.
///
/// # Examples
///
/// ```
/// let mut set = nc::cpu_set_t::default();
/// assert!(set.set(1).is_ok());
/// println!("set(1): {:?}", set.is_set(1));
/// assert!(set.set(2).is_ok());
/// assert!(set.clear(2).is_ok());
/// println!("set(2): {:?}", set.is_set(2));
///
/// let ret = unsafe { nc::sched_setaffinity(0, &set) };
/// assert!(ret.is_ok());
///
/// let mut set2 = nc::cpu_set_t::default();
/// let ret = unsafe { nc::sched_getaffinity(0, &mut set2) };
/// assert!(ret.is_ok());
/// assert_eq!(set, set2);
/// ```
pub unsafe fn sched_getaffinity(pid: pid_t, user_mask: &mut cpu_set_t) -> Result<(), Errno> {
    let pid = pid as usize;
    let cpu_set_len = core::mem::size_of::<cpu_set_t>();
    let user_mask_ptr = user_mask as *mut cpu_set_t as usize;
    syscall3(SYS_SCHED_GETAFFINITY, pid, cpu_set_len, user_mask_ptr).map(drop)
}

/// Get scheduling policy and attributes
pub unsafe fn sched_getattr(pid: pid_t, attr: &mut sched_attr_t, flags: u32) -> Result<(), Errno> {
    let pid = pid as usize;
    let attr_ptr = attr as *mut sched_attr_t as usize;
    let size = core::mem::size_of::<sched_attr_t>();
    let flags = flags as usize;
    syscall4(SYS_SCHED_GETATTR, pid, attr_ptr, size, flags).map(drop)
}

/// Get scheduling paramters.
///
/// # Examples
///
/// ```
/// let mut param = nc::sched_param_t::default();
/// let ret = unsafe { nc::sched_getparam(0, &mut param) };
/// assert!(ret.is_ok());
/// assert_eq!(param.sched_priority, 0);
/// ```
pub unsafe fn sched_getparam(pid: pid_t, param: &mut sched_param_t) -> Result<(), Errno> {
    let pid = pid as usize;
    let param_ptr = param as *mut sched_param_t as usize;
    syscall2(SYS_SCHED_GETPARAM, pid, param_ptr).map(drop)
}

/// Get scheduling parameter.
///
/// # Examples
///
/// ```
/// let ret = unsafe { nc::sched_getscheduler(0) };
/// assert_eq!(ret, Ok(nc::SCHED_NORMAL));
/// ```
pub unsafe fn sched_getscheduler(pid: pid_t) -> Result<i32, Errno> {
    let pid = pid as usize;
    syscall1(SYS_SCHED_GETSCHEDULER, pid).map(|ret| ret as i32)
}

/// Get static priority max value.
///
/// # Examples
///
/// ```
/// let ret = unsafe { nc::sched_get_priority_max(nc::SCHED_RR) };
/// assert!(ret.is_ok());
/// let max_prio = ret.unwrap();
/// assert_eq!(max_prio, 99);
/// ```
pub unsafe fn sched_get_priority_max(policy: i32) -> Result<i32, Errno> {
    let policy = policy as usize;
    syscall1(SYS_SCHED_GET_PRIORITY_MAX, policy).map(|ret| ret as i32)
}

/// Get static priority min value.
///
/// # Examples
///
/// ```
/// let ret = unsafe { nc::sched_get_priority_min(nc::SCHED_RR) };
/// assert!(ret.is_ok());
/// let min_prio = ret.unwrap();
/// assert_eq!(min_prio, 1);
/// ```
pub unsafe fn sched_get_priority_min(policy: i32) -> Result<i32, Errno> {
    let policy = policy as usize;
    syscall1(SYS_SCHED_GET_PRIORITY_MIN, policy).map(|ret| ret as i32)
}

/// Get the `SCHED_RR` interval for the named process.
///
/// # Examples
///
/// ```
/// let mut ts = nc::timespec_t::default();
/// let ret = unsafe { nc::sched_rr_get_interval(0, &mut ts) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn sched_rr_get_interval(pid: pid_t, interval: &mut timespec_t) -> Result<(), Errno> {
    let pid = pid as usize;
    let interval_ptr = interval as *mut timespec_t as usize;
    syscall2(SYS_SCHED_RR_GET_INTERVAL, pid, interval_ptr).map(drop)
}

/// Set a thread's CPU affinity mask.
///
/// # Examples
///
/// ```
/// let mut set = nc::cpu_set_t::default();
/// assert!(set.set(1).is_ok());
/// println!("set(1): {:?}", set.is_set(1));
/// assert!(set.set(2).is_ok());
/// assert!(set.clear(2).is_ok());
/// println!("set(2): {:?}", set.is_set(2));
///
/// let ret = unsafe { nc::sched_setaffinity(0, &set) };
/// assert!(ret.is_ok());
///
/// let mut set2 = nc::cpu_set_t::default();
/// let ret = unsafe { nc::sched_getaffinity(0, &mut set2) };
/// assert!(ret.is_ok());
/// assert_eq!(set, set2);
/// ```
pub unsafe fn sched_setaffinity(pid: pid_t, user_mask: &cpu_set_t) -> Result<(), Errno> {
    let pid = pid as usize;
    let cpu_set_len = core::mem::size_of::<cpu_set_t>();
    let user_mask_ptr = user_mask as *const cpu_set_t as usize;
    syscall3(SYS_SCHED_SETAFFINITY, pid, cpu_set_len, user_mask_ptr).map(drop)
}

/// Set the RT priority of a thread.
pub unsafe fn sched_setattr(pid: pid_t, attr: &sched_attr_t, flags: u32) -> Result<(), Errno> {
    let pid = pid as usize;
    let attr_ptr = attr as *const sched_attr_t as usize;
    let flags = flags as usize;
    syscall3(SYS_SCHED_SETATTR, pid, attr_ptr, flags).map(drop)
}

/// Set scheduling paramters.
///
/// # Examples
///
/// ```
/// // This call always returns error because default scheduler is SCHED_NORMAL.
/// // We shall call sched_setscheduler() and change to realtime policy
/// // like SCHED_RR or SCHED_FIFO.
/// let sched_param = nc::sched_param_t { sched_priority: 12 };
/// let ret = unsafe { nc::sched_setparam(0, &sched_param) };
/// assert_eq!(ret, Err(nc::EINVAL));
/// ```
pub unsafe fn sched_setparam(pid: pid_t, param: &sched_param_t) -> Result<(), Errno> {
    let pid = pid as usize;
    let param_ptr = param as *const sched_param_t as usize;
    syscall2(SYS_SCHED_SETPARAM, pid, param_ptr).map(drop)
}

/// Set scheduling parameter.
///
/// # Examples
///
/// ```
/// let sched_param = nc::sched_param_t { sched_priority: 12 };
/// let ret = unsafe { nc::sched_setscheduler(0, nc::SCHED_RR, &sched_param) };
/// assert_eq!(ret, Err(nc::EPERM));
/// ```
pub unsafe fn sched_setscheduler(
    pid: pid_t,
    policy: i32,
    param: &sched_param_t,
) -> Result<(), Errno> {
    let pid = pid as usize;
    let policy = policy as usize;
    let param_ptr = param as *const sched_param_t as usize;
    syscall3(SYS_SCHED_SETSCHEDULER, pid, policy, param_ptr).map(drop)
}

/// Yield the processor.
///
/// # Examples
///
/// ```
/// let ret = unsafe { nc::sched_yield() };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn sched_yield() -> Result<(), Errno> {
    syscall0(SYS_SCHED_YIELD).map(drop)
}

/// Operate on Secure Computing state of the process.
pub unsafe fn seccomp(
    operation: u32,
    flags: u32,
    args: *const core::ffi::c_void,
) -> Result<(), Errno> {
    let operation = operation as usize;
    let flags = flags as usize;
    let args = args as usize;
    syscall3(SYS_SECCOMP, operation, flags, args).map(drop)
}

/// System V semaphore control operations
///
/// # Examples
///
/// ```
/// use std::thread;
/// use std::time::Duration;
///
/// pub struct BinarySemaphore {
///     key: i32,
///     semid: i32,
///     is_producer: bool,
/// }
///
/// impl BinarySemaphore {
///     const SEM_NUM: i32 = 0;
///     const FIRST_SEM: u16 = 0;
///     const SEM_MAX_VAL: i32 = 1;
///     const SEM_OP_RESERVE: i16 = -1;
///     const SEM_OP_RELEASE: i16 = 1;
///
///     pub fn new(key: i32, is_producer: bool) -> Result<Self, nc::Errno> {
///         let semid =
///             unsafe { nc::semget(key, 1, nc::IPC_CREAT | (nc::S_IRUSR | nc::S_IWUSR) as i32)? };
///
///         let ret = unsafe {
///             let arg = Self::SEM_MAX_VAL as usize;
///             nc::semctl(semid, Self::SEM_NUM, nc::SETVAL, arg)
///         };
///
///         if let Err(errno) = ret {
///             let _ret = unsafe { nc::semctl(semid, nc::IPC_RMID, 0, 0) };
///             return Err(errno);
///         }
///
///         Ok(Self {
///             key,
///             semid,
///             is_producer,
///         })
///     }
///
///     pub fn reserve(&mut self) -> Result<(), nc::Errno> {
///         let mut ops = [nc::sembuf_t {
///             sem_num: Self::FIRST_SEM,
///             sem_op: Self::SEM_OP_RESERVE,
///             sem_flg: 0,
///         }];
///
///         loop {
///             let ret = unsafe { nc::semop(self.semid, &mut ops) };
///             match ret {
///                 Ok(_) => return Ok(()),
///                 Err(nc::EINTR) => continue,
///                 Err(errno) => return Err(errno),
///             }
///         }
///     }
///
///     pub fn release(&mut self) -> Result<(), nc::Errno> {
///         let mut ops = [nc::sembuf_t {
///             sem_num: Self::FIRST_SEM,
///             sem_op: Self::SEM_OP_RELEASE,
///             sem_flg: 0,
///         }];
///         unsafe { nc::semop(self.semid, &mut ops) }
///     }
///
///     #[must_use]
///     #[inline]
///     pub const fn key(&self) -> i32 {
///         self.key
///     }
///
///     #[must_use]
///     #[inline]
///     pub const fn is_producer(&self) -> bool {
///         self.is_producer
///     }
/// }
///
/// impl Drop for BinarySemaphore {
///     fn drop(&mut self) {
///         if self.is_producer {
///             let ret = unsafe { nc::semctl(self.semid, nc::IPC_RMID, 0, 0) };
///             assert!(ret.is_ok());
///         }
///     }
/// }
///
/// const KEY_ID: i32 = 0x1234;
/// let ret = BinarySemaphore::new(KEY_ID, true);
/// if let Err(errno) = ret {
///     eprintln!("sem init failed: {}", nc::strerror(errno));
///     return;
/// }
/// let mut sem = ret.unwrap();
///
/// // child thread as consumer
/// let handle = thread::spawn(|| {
///     let mut sem = BinarySemaphore::new(KEY_ID, false).unwrap();
///     for _ in 0..5 {
///         if let Err(errno) = sem.reserve() {
///             eprintln!("[worker ]sem reserve failed: {}", nc::strerror(errno));
///             break;
///         }
///         println!("[worker] wait for 100 millis");
///         thread::sleep(Duration::from_millis(100));
///         if let Err(errno) = sem.release() {
///             eprintln!("[worker] sem release failed: {}", nc::strerror(errno));
///         }
///     }
/// });
///
/// // parent thread as producer
/// for _ in 0..5 {
///     if let Err(errno) = sem.reserve() {
///         eprintln!("[worker ]sem reserve failed: {}", nc::strerror(errno));
///         break;
///     }
///     println!("[main] wait for 200 millis");
///     thread::sleep(Duration::from_millis(200));
///     if let Err(errno) = sem.release() {
///         eprintln!("[main] sem release failed: {}", nc::strerror(errno));
///     }
/// }
///
/// let _ = handle.join();
/// ```
pub unsafe fn semctl(semid: i32, semnum: i32, cmd: i32, arg: usize) -> Result<i32, Errno> {
    let semid = semid as usize;
    let semnum = semnum as usize;
    let cmd = cmd as usize;
    syscall4(SYS_SEMCTL, semid, semnum, cmd, arg).map(|ret| ret as i32)
}

/// Get a System V semphore set identifier.
///
/// # Examples
///
/// ```
/// use std::thread;
/// use std::time::Duration;
///
/// pub struct BinarySemaphore {
///     key: i32,
///     semid: i32,
///     is_producer: bool,
/// }
///
/// impl BinarySemaphore {
///     const SEM_NUM: i32 = 0;
///     const FIRST_SEM: u16 = 0;
///     const SEM_MAX_VAL: i32 = 1;
///     const SEM_OP_RESERVE: i16 = -1;
///     const SEM_OP_RELEASE: i16 = 1;
///
///     pub fn new(key: i32, is_producer: bool) -> Result<Self, nc::Errno> {
///         let semid =
///             unsafe { nc::semget(key, 1, nc::IPC_CREAT | (nc::S_IRUSR | nc::S_IWUSR) as i32)? };
///
///         let ret = unsafe {
///             let arg = Self::SEM_MAX_VAL as usize;
///             nc::semctl(semid, Self::SEM_NUM, nc::SETVAL, arg)
///         };
///
///         if let Err(errno) = ret {
///             let _ret = unsafe { nc::semctl(semid, nc::IPC_RMID, 0, 0) };
///             return Err(errno);
///         }
///
///         Ok(Self {
///             key,
///             semid,
///             is_producer,
///         })
///     }
///
///     pub fn reserve(&mut self) -> Result<(), nc::Errno> {
///         let mut ops = [nc::sembuf_t {
///             sem_num: Self::FIRST_SEM,
///             sem_op: Self::SEM_OP_RESERVE,
///             sem_flg: 0,
///         }];
///
///         loop {
///             let ret = unsafe { nc::semop(self.semid, &mut ops) };
///             match ret {
///                 Ok(_) => return Ok(()),
///                 Err(nc::EINTR) => continue,
///                 Err(errno) => return Err(errno),
///             }
///         }
///     }
///
///     pub fn release(&mut self) -> Result<(), nc::Errno> {
///         let mut ops = [nc::sembuf_t {
///             sem_num: Self::FIRST_SEM,
///             sem_op: Self::SEM_OP_RELEASE,
///             sem_flg: 0,
///         }];
///         unsafe { nc::semop(self.semid, &mut ops) }
///     }
///
///     #[must_use]
///     #[inline]
///     pub const fn key(&self) -> i32 {
///         self.key
///     }
///
///     #[must_use]
///     #[inline]
///     pub const fn is_producer(&self) -> bool {
///         self.is_producer
///     }
/// }
///
/// impl Drop for BinarySemaphore {
///     fn drop(&mut self) {
///         if self.is_producer {
///             let ret = unsafe { nc::semctl(self.semid, nc::IPC_RMID, 0, 0) };
///             assert!(ret.is_ok());
///         }
///     }
/// }
///
/// const KEY_ID: i32 = 0x1235;
/// let ret = BinarySemaphore::new(KEY_ID, true);
/// if let Err(errno) = ret {
///     eprintln!("sem init failed: {}", nc::strerror(errno));
///     return;
/// }
/// let mut sem = ret.unwrap();
///
/// // child thread as consumer
/// let handle = thread::spawn(|| {
///     let mut sem = BinarySemaphore::new(KEY_ID, false).unwrap();
///     for _ in 0..5 {
///         if let Err(errno) = sem.reserve() {
///             eprintln!("[worker ]sem reserve failed: {}", nc::strerror(errno));
///             break;
///         }
///         println!("[worker] wait for 100 millis");
///         thread::sleep(Duration::from_millis(100));
///         if let Err(errno) = sem.release() {
///             eprintln!("[worker] sem release failed: {}", nc::strerror(errno));
///         }
///     }
/// });
///
/// // parent thread as producer
/// for _ in 0..5 {
///     if let Err(errno) = sem.reserve() {
///         eprintln!("[worker ]sem reserve failed: {}", nc::strerror(errno));
///         break;
///     }
///     println!("[main] wait for 200 millis");
///     thread::sleep(Duration::from_millis(200));
///     if let Err(errno) = sem.release() {
///         eprintln!("[main] sem release failed: {}", nc::strerror(errno));
///     }
/// }
///
/// let _ = handle.join();
/// ```
pub unsafe fn semget(key: key_t, nsems: i32, sem_flag: i32) -> Result<i32, Errno> {
    let key = key as usize;
    let nsems = nsems as usize;
    let sem_flag = sem_flag as usize;
    syscall3(SYS_SEMGET, key, nsems, sem_flag).map(|ret| ret as i32)
}

/// System V semphore operations.
///
/// # Examples
///
/// ```
/// use std::thread;
/// use std::time::Duration;
///
/// pub struct BinarySemaphore {
///     key: i32,
///     semid: i32,
///     is_producer: bool,
/// }
///
/// impl BinarySemaphore {
///     const SEM_NUM: i32 = 0;
///     const FIRST_SEM: u16 = 0;
///     const SEM_MAX_VAL: i32 = 1;
///     const SEM_OP_RESERVE: i16 = -1;
///     const SEM_OP_RELEASE: i16 = 1;
///
///     pub fn new(key: i32, is_producer: bool) -> Result<Self, nc::Errno> {
///         let semid =
///             unsafe { nc::semget(key, 1, nc::IPC_CREAT | (nc::S_IRUSR | nc::S_IWUSR) as i32)? };
///
///         let ret = unsafe {
///             let arg = Self::SEM_MAX_VAL as usize;
///             nc::semctl(semid, Self::SEM_NUM, nc::SETVAL, arg)
///         };
///
///         if let Err(errno) = ret {
///             let _ret = unsafe { nc::semctl(semid, nc::IPC_RMID, 0, 0) };
///             return Err(errno);
///         }
///
///         Ok(Self {
///             key,
///             semid,
///             is_producer,
///         })
///     }
///
///     pub fn reserve(&mut self) -> Result<(), nc::Errno> {
///         let mut ops = [nc::sembuf_t {
///             sem_num: Self::FIRST_SEM,
///             sem_op: Self::SEM_OP_RESERVE,
///             sem_flg: 0,
///         }];
///
///         loop {
///             let ret = unsafe { nc::semop(self.semid, &mut ops) };
///             match ret {
///                 Ok(_) => return Ok(()),
///                 Err(nc::EINTR) => continue,
///                 Err(errno) => return Err(errno),
///             }
///         }
///     }
///
///     pub fn release(&mut self) -> Result<(), nc::Errno> {
///         let mut ops = [nc::sembuf_t {
///             sem_num: Self::FIRST_SEM,
///             sem_op: Self::SEM_OP_RELEASE,
///             sem_flg: 0,
///         }];
///         unsafe { nc::semop(self.semid, &mut ops) }
///     }
///
///     #[must_use]
///     #[inline]
///     pub const fn key(&self) -> i32 {
///         self.key
///     }
///
///     #[must_use]
///     #[inline]
///     pub const fn is_producer(&self) -> bool {
///         self.is_producer
///     }
/// }
///
/// impl Drop for BinarySemaphore {
///     fn drop(&mut self) {
///         if self.is_producer {
///             let ret = unsafe { nc::semctl(self.semid, nc::IPC_RMID, 0, 0) };
///             assert!(ret.is_ok());
///         }
///     }
/// }
///
/// const KEY_ID: i32 = 0x1236;
/// let ret = BinarySemaphore::new(KEY_ID, true);
/// if let Err(errno) = ret {
///     eprintln!("sem init failed: {}", nc::strerror(errno));
///     return;
/// }
/// let mut sem = ret.unwrap();
///
/// // child thread as consumer
/// let handle = thread::spawn(|| {
///     let mut sem = BinarySemaphore::new(KEY_ID, false).unwrap();
///     for _ in 0..5 {
///         if let Err(errno) = sem.reserve() {
///             eprintln!("[worker ]sem reserve failed: {}", nc::strerror(errno));
///             break;
///         }
///         println!("[worker] wait for 100 millis");
///         thread::sleep(Duration::from_millis(100));
///         if let Err(errno) = sem.release() {
///             eprintln!("[worker] sem release failed: {}", nc::strerror(errno));
///         }
///     }
/// });
///
/// // parent thread as producer
/// for _ in 0..5 {
///     if let Err(errno) = sem.reserve() {
///         eprintln!("[worker ]sem reserve failed: {}", nc::strerror(errno));
///         break;
///     }
///     println!("[main] wait for 200 millis");
///     thread::sleep(Duration::from_millis(200));
///     if let Err(errno) = sem.release() {
///         eprintln!("[main] sem release failed: {}", nc::strerror(errno));
///     }
/// }
///
/// let _ = handle.join();
/// ```
pub unsafe fn semop(semid: i32, sops: &mut [sembuf_t]) -> Result<(), Errno> {
    let semid = semid as usize;
    let sops_ptr = sops.as_mut_ptr() as usize;
    let nops = sops.len();
    syscall3(SYS_SEMOP, semid, sops_ptr, nops).map(drop)
}

/// System V semaphore operations
pub unsafe fn semtimedop(
    semid: i32,
    sops: &mut [sembuf_t],
    timeout: Option<&timespec_t>,
) -> Result<(), Errno> {
    let semid = semid as usize;
    let sops_ptr = sops.as_mut_ptr() as usize;
    let nops = sops.len();
    let timeout_ptr = timeout.map_or(core::ptr::null::<timespec_t>() as usize, |timeout| {
        timeout as *const timespec_t as usize
    });
    syscall4(SYS_SEMTIMEDOP, semid, sops_ptr, nops, timeout_ptr).map(drop)
}

/// Transfer data between two file descriptors.
///
/// # Examples
///
/// ```
/// println!("S_IRUSR: {}", nc::S_IRUSR);
/// println!("S_IRGRP: {}", nc::S_IRGRP);
///
/// let in_filename = "/etc/passwd";
/// let out_filename = "/tmp/passwd.copy";
///
/// #[cfg(any(target_os = "linux", target_os = "android"))]
/// let in_fd = unsafe {
///     nc::openat(nc::AT_FDCWD, in_filename, nc::O_RDONLY, 0).expect("Failed to open file!")
/// };
///
/// #[cfg(target_os = "freebsd")]
/// let in_fd = unsafe {
///     nc::openat(nc::AT_FDCWD, in_filename, nc::O_RDONLY, 0).expect("Failed to open file!")
/// };
///
/// #[cfg(any(target_os = "linux", target_os = "android"))]
/// let out_fd = unsafe {
///     nc::openat(
///         nc::AT_FDCWD,
///         out_filename,
///         nc::O_WRONLY | nc::O_CREAT,
///         nc::S_IRUSR | nc::S_IWUSR | nc::S_IRGRP | nc::S_IROTH,
///     )
///     .expect("Failed to open passwd copy file")
/// };
///
/// #[cfg(target_os = "freebsd")]
/// let out_fd = unsafe {
///     nc::openat(
///         nc::AT_FDCWD,
///         out_filename,
///         nc::O_WRONLY | nc::O_CREAT,
///         nc::S_IRUSR | nc::S_IWUSR | nc::S_IRGRP | nc::S_IROTH,
///     )
///     .expect("Failed to open passwd copy file")
/// };
///
/// let mut stat = nc::stat_t::default();
/// unsafe { nc::fstat(in_fd, &mut stat).expect("Failed to get file stat!") };
/// println!("stat: {:?}", stat);
///
/// let count = stat.st_blksize as usize;
/// println!("count: {}", count);
/// let nread =
///     unsafe { nc::sendfile(out_fd, in_fd, None, count).expect("Failed to call sendfile()") };
/// println!("nread: {}", nread);
///
/// unsafe {
///     let _ = nc::close(in_fd);
///     let _ = nc::close(out_fd);
/// }
///
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, out_filename, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn sendfile(
    out_fd: i32,
    in_fd: i32,
    offset: Option<&mut off_t>,
    count: size_t,
) -> Result<ssize_t, Errno> {
    let out_fd = out_fd as usize;
    let in_fd = in_fd as usize;
    let offset_ptr = offset.map_or(core::ptr::null_mut::<off_t>() as usize, |offset| {
        offset as *mut off_t as usize
    });
    syscall4(SYS_SENDFILE, out_fd, in_fd, offset_ptr, count).map(|ret| ret as ssize_t)
}

/// Send multiple messages on a socket
///
/// Returns number of messages sent.
///
/// # Examples
/// ```
/// use std::ffi::c_void;
/// use std::ptr;
/// use std::thread;
///
/// const READ_SIDE: usize = 0;
/// const WRITE_SIDE: usize = 1;
///
/// fn main() {
///     let mut fds = [-1_i32; 2];
///
///     let ret = unsafe { nc::socketpair(nc::AF_UNIX, nc::SOCK_STREAM, 0, &mut fds) };
///     assert!(ret.is_ok());
///     println!("socket pairs: {}, {}", fds[0], fds[1]);
///
///     // Start worker thread
///     thread::spawn(move || {
///         println!("worker thread started");
///         let msg = "Hello, Rust";
///         println!("[worker] Will send msg: {msg}");
///         let mut iov = [nc::iovec_t {
///             iov_base: msg.as_ptr() as *const c_void,
///             iov_len: msg.len(),
///         }];
///         let msg_hdr = nc::msghdr_t {
///             msg_name: ptr::null(),
///             msg_namelen: 0,
///             msg_iov: iov.as_mut_ptr(),
///             msg_iovlen: iov.len(),
///             msg_control: ptr::null(),
///             msg_controllen: 0,
///             msg_flags: 0,
///         };
///         let mmsg_hdr = [nc::mmsghdr_t {
///             msg_hdr,
///             msg_len: 0,
///         }];
///         let ret = unsafe { nc::sendmmsg(fds[WRITE_SIDE], &mmsg_hdr, 0) };
///         assert!(ret.is_ok());
///         //assert_eq!(ret, Ok(msg.len() as nc::ssize_t));
///     });
///
///     let mut buf = [[0_u8; 64]; 4];
///     let mut iov = Vec::with_capacity(buf.len());
///     for ref mut item in (&mut buf).iter() {
///         iov.push(nc::iovec_t {
///             iov_base: item.as_ptr() as *const c_void,
///             iov_len: item.len(),
///         });
///     }
///     let msg_hdr = nc::msghdr_t {
///         msg_name: ptr::null(),
///         msg_namelen: 0,
///         msg_iov: iov.as_mut_ptr(),
///         msg_iovlen: iov.len(),
///         msg_control: ptr::null(),
///         msg_controllen: 0,
///         msg_flags: 0,
///     };
///     let mut mmsg_hdr = [nc::mmsghdr_t {
///         msg_hdr,
///         msg_len: 0,
///     }];
///     let ret = unsafe { nc::recvmmsg(fds[READ_SIDE], &mut mmsg_hdr, 0, None) };
///     assert!(ret.is_ok());
///     let msgs_read = ret.unwrap();
///     assert_eq!(msgs_read, 1);
///     // We only read the first buffer block.
///     for i in 0..msgs_read {
///         let nread = mmsg_hdr[i].msg_len;
///         let msg = std::str::from_utf8(&buf[0][..nread as usize]).unwrap();
///         println!("[main] recv msg: {msg}");
///     }
///
///     unsafe {
///         let _ = nc::close(fds[0]);
///         let _ = nc::close(fds[1]);
///     }
/// }
/// ```
pub unsafe fn sendmmsg(sockfd: i32, msgvec: &[mmsghdr_t], flags: i32) -> Result<size_t, Errno> {
    let sockfd = sockfd as usize;
    let msgvec_ptr = msgvec.as_ptr() as usize;
    let vlen = msgvec.len();
    let flags = flags as usize;
    syscall4(SYS_SENDMMSG, sockfd, msgvec_ptr, vlen, flags)
}

/// Send a message on a socket.
///
/// Allow sending ancillary data.
///
/// # Examples
///
/// ```
/// use std::ffi::c_void;
/// use std::ptr;
/// use std::thread;
///
/// const READ_SIDE: usize = 0;
/// const WRITE_SIDE: usize = 1;
///
/// fn main() {
///     let mut fds = [-1_i32; 2];
///
///     let ret = unsafe { nc::socketpair(nc::AF_UNIX, nc::SOCK_STREAM, 0, &mut fds) };
///     assert!(ret.is_ok());
///     println!("socket pairs: {}, {}", fds[0], fds[1]);
///
///     // Start worker thread
///     thread::spawn(move || {
///         println!("worker thread started");
///         let msg = "Hello, Rust";
///         println!("[worker] Will send msg: {msg}");
///         let mut iov = [nc::iovec_t {
///             iov_base: msg.as_ptr() as *const c_void,
///             iov_len: msg.len(),
///         }];
///         let msg_hdr = nc::msghdr_t {
///             msg_name: ptr::null(),
///             msg_namelen: 0,
///             msg_iov: iov.as_mut_ptr(),
///             msg_iovlen: iov.len(),
///             msg_control: ptr::null(),
///             msg_controllen: 0,
///             msg_flags: 0,
///         };
///         let ret = unsafe { nc::sendmsg(fds[WRITE_SIDE], &msg_hdr, 0) };
///         assert!(ret.is_ok());
///         assert_eq!(ret, Ok(msg.len() as nc::ssize_t));
///     });
///
///     let mut buf = [[0_u8; 64]; 4];
///     let mut iov = Vec::with_capacity(buf.len());
///     for ref mut item in (&mut buf).iter() {
///         iov.push(nc::iovec_t {
///             iov_base: item.as_ptr() as *const c_void,
///             iov_len: item.len(),
///         });
///     }
///     let mut msg_hdr = nc::msghdr_t {
///         msg_name: ptr::null(),
///         msg_namelen: 0,
///         msg_iov: iov.as_mut_ptr(),
///         msg_iovlen: iov.len(),
///         msg_control: ptr::null(),
///         msg_controllen: 0,
///         msg_flags: 0,
///     };
///     let ret = unsafe { nc::recvmsg(fds[READ_SIDE], &mut msg_hdr, 0) };
///     assert!(ret.is_ok());
///     let nread = ret.unwrap();
///     // We only read the first buffer block.
///     let msg = std::str::from_utf8(&buf[0][..nread as usize]).unwrap();
///     println!("[main] recv msg: {msg}");
///
///     unsafe {
///         let _ = nc::close(fds[0]);
///         let _ = nc::close(fds[1]);
///     }
/// }
/// ```
pub unsafe fn sendmsg(sockfd: i32, msg: &msghdr_t, flags: i32) -> Result<ssize_t, Errno> {
    let sockfd = sockfd as usize;
    let msg_ptr = msg as *const msghdr_t as usize;
    let flags = flags as usize;
    syscall3(SYS_SENDMSG, sockfd, msg_ptr, flags).map(|ret| ret as ssize_t)
}

/// Send a message on a socket.
///
/// # Examples
///
/// ```
/// use std::thread;
///
/// const READ_SIDE: usize = 0;
/// const WRITE_SIDE: usize = 1;
///
/// fn main() {
///     let mut fds = [-1_i32; 2];
///
///     let ret = unsafe { nc::socketpair(nc::AF_UNIX, nc::SOCK_STREAM, 0, &mut fds) };
///     assert!(ret.is_ok());
///     println!("socket pairs: {}, {}", fds[0], fds[1]);
///
///     // Start worker thread
///     thread::spawn(move || {
///         println!("worker thread started");
///         let msg = "Hello, Rust";
///         println!("[worker] Will send msg: {msg}");
///         let ret = unsafe { nc::sendto(fds[WRITE_SIDE], msg.as_bytes(), 0, None, 0) };
///         assert!(ret.is_ok());
///         assert_eq!(ret, Ok(msg.len() as nc::ssize_t));
///     });
///
///     let mut buf = [0_u8; 1024];
///     let ret = unsafe { nc::recvfrom(fds[READ_SIDE], &mut buf, 0, None, None) };
///     assert!(ret.is_ok());
///     let nread = ret.unwrap();
///     let msg = std::str::from_utf8(&buf[..nread as usize]).unwrap();
///     println!("[main] recv msg: {msg}");
///
///     unsafe {
///         let _ = nc::close(fds[0]);
///         let _ = nc::close(fds[1]);
///     }
/// }
/// ```
pub unsafe fn sendto(
    sockfd: i32,
    buf: &[u8],
    flags: i32,
    dest_addr: Option<*const sockaddr_t>,
    addrlen: socklen_t,
) -> Result<ssize_t, Errno> {
    let sockfd = sockfd as usize;
    let buf_ptr = buf.as_ptr() as usize;
    let len = buf.len();
    let flags = flags as usize;
    let dest_addr_ptr = dest_addr.map_or(core::ptr::null::<sockaddr_t>() as usize, |dest_addr| {
        dest_addr as usize
    });
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

/// Set NIS domain name.
///
/// # Examples
///
/// ```
/// let name = "local-rust-domain";
/// let ret = unsafe { nc::setdomainname(name) };
/// assert!(ret.is_err());
/// assert_eq!(ret, Err(nc::EPERM));
/// ```
pub unsafe fn setdomainname<P: AsRef<Path>>(name: P) -> Result<(), Errno> {
    let name = CString::new(name.as_ref());
    let name_ptr = name.as_ptr() as usize;
    let name_len = name.len();
    syscall2(SYS_SETDOMAINNAME, name_ptr, name_len).map(drop)
}

/// Set group identify used for filesystem checkes.
///
/// # Examples
///
/// ```
/// let ret = unsafe { nc::setfsgid(0) };
/// assert!(ret.is_ok());
/// let gid = unsafe { nc::getgid() };
/// assert_eq!(ret, Ok(gid));
/// ```
pub unsafe fn setfsgid(fsgid: gid_t) -> Result<gid_t, Errno> {
    let fsgid = fsgid as usize;
    syscall1(SYS_SETFSGID, fsgid).map(|ret| ret as gid_t)
}

/// Set user identify used for filesystem checkes.
///
/// # Examples
///
/// ```
/// let ret = unsafe { nc::setfsuid(0) };
/// assert!(ret.is_ok());
/// let uid = unsafe { nc::getuid() };
/// assert_eq!(ret, Ok(uid));
/// ```
pub unsafe fn setfsuid(fsuid: uid_t) -> Result<uid_t, Errno> {
    let fsuid = fsuid as usize;
    syscall1(SYS_SETFSUID, fsuid).map(|ret| ret as uid_t)
}

/// Set the group ID of the calling process to `gid`.
///
/// # Examples
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
/// # Examples
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

/// Set hostname.
///
/// # Exampless
///
/// ```
/// let name = "rust-machine";
/// let ret = unsafe { nc::sethostname(name) };
/// assert!(ret.is_err());
/// assert_eq!(ret, Err(nc::EPERM));
/// ```
pub unsafe fn sethostname<P: AsRef<Path>>(name: P) -> Result<(), Errno> {
    let name = CString::new(name.as_ref());
    let name_ptr = name.as_ptr() as usize;
    let name_len = name.len();
    syscall2(SYS_SETHOSTNAME, name_ptr, name_len).map(drop)
}

/// Set value of an interval timer.
///
/// # Examples
///
/// ```
/// fn handle_alarm(signum: i32) {
///     assert_eq!(signum, nc::SIGALRM);
///     let msg = b"Hello alarm\n";
///     let stderr = 2;
///     let _ = unsafe { nc::write(stderr, msg) };
/// }
///
/// let sa = nc::new_sigaction(handle_alarm);
/// let ret = unsafe { nc::rt_sigaction(nc::SIGALRM, Some(&sa), None) };
/// assert!(ret.is_ok());
///
/// // Single shot timer, actived after 1 second.
/// let itv = nc::itimerval_t {
///     it_value: nc::timeval_t {
///         tv_sec: 1,
///         tv_usec: 0,
///     },
///     it_interval: nc::timeval_t {
///         tv_sec: 0,
///         tv_usec: 0,
///     },
/// };
/// let ret = unsafe { nc::setitimer(nc::ITIMER_REAL, &itv, None) };
/// assert!(ret.is_ok());
///
/// let mut prev_itv = nc::itimerval_t::default();
/// let ret = unsafe { nc::getitimer(nc::ITIMER_REAL, &mut prev_itv) };
/// assert!(ret.is_ok());
/// assert!(prev_itv.it_value.tv_sec <= itv.it_value.tv_sec);
///
/// let mask = nc::sigset_t::default();
/// let _ret = unsafe { nc::rt_sigsuspend(&mask) };
///
/// let ret = unsafe { nc::getitimer(nc::ITIMER_REAL, &mut prev_itv) };
/// assert!(ret.is_ok());
/// assert_eq!(prev_itv.it_value.tv_sec, 0);
/// assert_eq!(prev_itv.it_value.tv_usec, 0);
/// ```
pub unsafe fn setitimer(
    which: i32,
    new_val: &itimerval_t,
    old_val: Option<&mut itimerval_t>,
) -> Result<(), Errno> {
    let which = which as usize;
    let new_val_ptr = new_val as *const itimerval_t as usize;
    let old_val_ptr = old_val.map_or(core::ptr::null_mut::<itimerval_t>() as usize, |old_val| {
        old_val as *mut itimerval_t as usize
    });
    syscall3(SYS_SETITIMER, which, new_val_ptr, old_val_ptr).map(drop)
}

/// Reassociate thread with a namespace.
pub unsafe fn setns(fd: i32, nstype: i32) -> Result<(), Errno> {
    let fd = fd as usize;
    let nstype = nstype as usize;
    syscall2(SYS_SETNS, fd, nstype).map(drop)
}

/// Set the process group ID (PGID) of the process specified by `pid` to `pgid`.
///
/// # Examples
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
/// # Examples
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
/// # Examples
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

/// Set real, effective and saved group Ids of the calling process.
///
/// # Examples
///
/// ```
/// let ret = unsafe { nc::setresgid(0, 0, 0) };
/// assert_eq!(ret, Err(nc::EPERM));
/// ```
pub unsafe fn setresgid(rgid: gid_t, egid: gid_t, sgid: gid_t) -> Result<(), Errno> {
    let rgid = rgid as usize;
    let egid = egid as usize;
    let sgid = sgid as usize;
    syscall3(SYS_SETRESGID, rgid, egid, sgid).map(drop)
}

/// Set real, effective and saved user Ids of the calling process.
///
/// # Examples
///
/// ```
/// let ret = unsafe { nc::setresuid(0, 0, 0) };
/// assert_eq!(ret, Err(nc::EPERM));
/// ```
pub unsafe fn setresuid(ruid: uid_t, euid: uid_t, suid: uid_t) -> Result<(), Errno> {
    let ruid = ruid as usize;
    let euid = euid as usize;
    let suid = suid as usize;
    syscall3(SYS_SETRESUID, ruid, euid, suid).map(drop)
}

/// Set real and effective user IDs of the calling process.
///
/// # Examples
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
/// # Examples
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
/// # Examples
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
/// # Examples
///
/// ```
/// let socket_fd = unsafe { nc::socket(nc::AF_INET, nc::SOCK_STREAM, 0) };
/// assert!(socket_fd.is_ok());
/// let socket_fd = socket_fd.unwrap();
///
/// // Enable tcp fast open.
/// let queue_len: i32 = 5;
/// let ret = unsafe {
///     nc::setsockopt(
///         socket_fd,
///         nc::IPPROTO_TCP,
///         nc::TCP_FASTOPEN,
///         &queue_len as *const i32 as *const _,
///         std::mem::size_of_val(&queue_len) as nc::socklen_t,
///     )
/// };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::close(socket_fd) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn setsockopt(
    sockfd: i32,
    level: i32,
    opt_name: i32,
    opt_val: *const core::ffi::c_void,
    opt_len: socklen_t,
) -> Result<(), Errno> {
    let sockfd = sockfd as usize;
    let level = level as usize;
    let opt_name = opt_name as usize;
    let opt_val = opt_val as usize;
    let opt_len = opt_len as usize;
    syscall5(SYS_SETSOCKOPT, sockfd, level, opt_name, opt_val, opt_len).map(drop)
}

/// Set system time and timezone.
///
/// ```
/// let tv = nc::timeval_t {
///     tv_sec: 1,
///     tv_usec: 0,
/// };
/// let ret = unsafe { nc::settimeofday(&tv, None) };
/// assert!(ret.is_err());
/// assert_eq!(ret, Err(nc::EPERM));
/// ```
pub unsafe fn settimeofday(timeval: &timeval_t, tz: Option<&timezone_t>) -> Result<(), Errno> {
    let timeval_ptr = timeval as *const timeval_t as usize;
    let tz_ptr = tz.map_or(core::ptr::null::<timezone_t>() as usize, |tz| {
        tz as *const timezone_t as usize
    });
    syscall2(SYS_SETTIMEOFDAY, timeval_ptr, tz_ptr).map(drop)
}

/// Set user ID of the calling process to `uid`.
///
/// # Examples
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
/// # Examples
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
///         attr_value.as_bytes(),
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
    value: &[u8],
    flags: i32,
) -> Result<(), Errno> {
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    let name = CString::new(name.as_ref());
    let name_ptr = name.as_ptr() as usize;
    let value_ptr = value.as_ptr() as usize;
    let size = value.len();
    let flags = flags as usize;
    syscall5(SYS_SETXATTR, filename_ptr, name_ptr, value_ptr, size, flags).map(drop)
}

/// Set default NUMA memory policy for a thread and its children
pub unsafe fn set_mempolicy(mode: i32, nmask: &[usize], max_node: usize) -> Result<(), Errno> {
    let mode = mode as usize;
    let nmask = nmask.as_ptr() as usize;
    syscall3(SYS_SET_MEMPOLICY, mode, nmask, max_node).map(drop)
}

pub unsafe fn set_mempolicy_home_node(
    start: *const core::ffi::c_void,
    len: usize,
    home_node: usize,
    flags: usize,
) -> Result<(), Errno> {
    let start = start as usize;
    syscall4(SYS_SET_MEMPOLICY_HOME_NODE, start, len, home_node, flags).map(drop)
}

/// Set the robust-futex list head of a task.
///
/// Params:
/// - `head`: pointer to the list-head
/// - `len`: length of the list-head, as userspace expects
pub unsafe fn set_robust_list(head: *mut robust_list_head_t, len: usize) -> Result<(), Errno> {
    let head_ptr = head as usize;
    syscall2(SYS_SET_ROBUST_LIST, head_ptr, len).map(drop)
}

/// Set pointer to thread ID.
///
/// Always returns the caller's thread id.
pub unsafe fn set_tid_address(tid: &mut i32) -> pid_t {
    let tid_ptr = tid as *mut i32 as usize;
    // This function is always successful.
    syscall1(SYS_SET_TID_ADDRESS, tid_ptr).unwrap_or_default() as pid_t
}

/// Attach the System V shared memory segment.
///
/// # Examples
///
/// ```
/// let size = 4 * nc::PAGE_SIZE;
/// let flags = nc::IPC_CREAT | nc::IPC_EXCL | 0o600;
/// let ret = unsafe { nc::shmget(nc::IPC_PRIVATE, size, flags) };
/// assert!(ret.is_ok());
/// let shmid = ret.unwrap();
///
/// let ret = unsafe { nc::shmat(shmid, None, 0) };
/// assert!(ret.is_ok());
/// let addr: *const std::ffi::c_void = ret.unwrap();
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
pub unsafe fn shmat(
    shmid: i32,
    shm_addr: Option<*const core::ffi::c_void>,
    shm_flag: i32,
) -> Result<*const core::ffi::c_void, Errno> {
    let shmid = shmid as usize;
    let shm_addr = shm_addr.map_or(core::ptr::null::<u8>() as usize, |shm_addr| {
        shm_addr as usize
    });
    let shm_flag = shm_flag as usize;
    syscall3(SYS_SHMAT, shmid, shm_addr, shm_flag).map(|ret| ret as *const core::ffi::c_void)
}

/// System V shared memory control.
///
/// # Examples
///
/// ```
/// let size = 4 * nc::PAGE_SIZE;
/// let flags = nc::IPC_CREAT | nc::IPC_EXCL | 0o600;
/// let ret = unsafe { nc::shmget(nc::IPC_PRIVATE, size, flags) };
/// assert!(ret.is_ok());
/// let shmid = ret.unwrap();
/// let mut buf = nc::shmid_ds_t::default();
/// let ret = unsafe { nc::shmctl(shmid, nc::IPC_RMID, &mut buf) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn shmctl(shmid: i32, cmd: i32, buf: &mut shmid_ds_t) -> Result<i32, Errno> {
    let shmid = shmid as usize;
    let cmd = cmd as usize;
    let buf_ptr = buf as *mut shmid_ds_t as usize;
    syscall3(SYS_SHMCTL, shmid, cmd, buf_ptr).map(|ret| ret as i32)
}

/// Detach the System V shared memory segment.
///
/// # Examples
///
/// ```
/// let size = 4 * nc::PAGE_SIZE;
/// let flags = nc::IPC_CREAT | nc::IPC_EXCL | 0o600;
/// let ret = unsafe { nc::shmget(nc::IPC_PRIVATE, size, flags) };
/// assert!(ret.is_ok());
/// let shmid = ret.unwrap();
///
/// let ret = unsafe { nc::shmat(shmid, None, 0) };
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
pub unsafe fn shmdt(shm_addr: *const core::ffi::c_void) -> Result<(), Errno> {
    let shm_addr = shm_addr as usize;
    syscall1(SYS_SHMDT, shm_addr).map(drop)
}

/// Allocates a System V shared memory segment.
///
/// # Examples
///
/// ```
/// let size = 4 * nc::PAGE_SIZE;
/// let flags = nc::IPC_CREAT | nc::IPC_EXCL | 0o600;
/// let ret = unsafe { nc::shmget(nc::IPC_PRIVATE, size, flags) };
/// assert!(ret.is_ok());
/// let shmid = ret.unwrap();
/// let mut buf = nc::shmid_ds_t::default();
/// let ret = unsafe { nc::shmctl(shmid, nc::IPC_RMID, &mut buf) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn shmget(key: key_t, size: size_t, shm_flag: i32) -> Result<i32, Errno> {
    let key = key as usize;
    let shm_flag = shm_flag as usize;
    syscall3(SYS_SHMGET, key, size, shm_flag).map(|ret| ret as i32)
}

/// Shutdown part of a full-duplex connection.
///
/// # Examples
///
/// ```
/// use nc::Errno;
/// use std::mem::{size_of, transmute};
/// use std::thread;
///
/// const SERVER_PORT: u16 = 18087;
///
/// #[must_use]
/// #[inline]
/// const fn htons(host: u16) -> u16 {
///     host.to_be()
/// }
///
/// fn main() -> Result<(), Errno> {
///     let listen_fd = unsafe { nc::socket(nc::AF_INET, nc::SOCK_STREAM, 0)? };
///     println!("listen fd: {listen_fd}");
///
///     let addr = nc::sockaddr_in_t {
///         sin_family: nc::AF_INET as nc::sa_family_t,
///         sin_port: htons(SERVER_PORT),
///         sin_addr: nc::in_addr_t {
///             s_addr: nc::INADDR_ANY as u32,
///         },
///         ..Default::default()
///     };
///     println!("addr: {addr:?}");
///
///     let ret = unsafe {
///         let addr_alias = transmute::<&nc::sockaddr_in_t, &nc::sockaddr_t>(&addr);
///         nc::bind(listen_fd, addr_alias, size_of::<nc::sockaddr_in_t>() as u32)
///     };
///     assert!(ret.is_ok());
///
///     // Start worker thread
///     thread::spawn(|| {
///         println!("worker thread started");
///         let socket_fd = unsafe { nc::socket(nc::AF_INET, nc::SOCK_STREAM, 0) };
///         assert!(socket_fd.is_ok());
///         if let Ok(socket_fd) = socket_fd {
///             let addr = nc::sockaddr_in_t {
///                 sin_family: nc::AF_INET as nc::sa_family_t,
///                 sin_port: htons(SERVER_PORT),
///                 sin_addr: nc::in_addr_t {
///                     s_addr: nc::INADDR_ANY as u32,
///                 },
///                 ..Default::default()
///             };
///             unsafe {
///                 let addr_alias = transmute::<&nc::sockaddr_in_t, &nc::sockaddr_t>(&addr);
///                 let ret = nc::connect(socket_fd, addr_alias, size_of::<nc::sockaddr_in_t>() as u32);
///                 assert_eq!(ret, Ok(()));
///
///                 let _ = nc::shutdown(socket_fd, nc::SHUT_RDWR);
///             }
///         } else {
///             eprintln!("Failed to create socket");
///         }
///     });
///
///     unsafe {
///         nc::listen(listen_fd, nc::SOCK_STREAM)?;
///     }
///
///     let conn_fd = unsafe {
///         nc::accept4(listen_fd, None, None, nc::SOCK_CLOEXEC)?
///     };
///     println!("conn_fd: {conn_fd}");
///
///     unsafe {
///         nc::close(listen_fd)?;
///     }
///
///     Ok(())
/// }
/// ```
pub unsafe fn shutdown(sockfd: i32, how: i32) -> Result<(), Errno> {
    let sockfd = sockfd as usize;
    let how = how as usize;
    syscall2(SYS_SHUTDOWN, sockfd, how).map(drop)
}

/// Get/set signal stack context.
pub unsafe fn sigaltstack(
    ss: Option<&sigaltstack_t>,
    old_ss: Option<&mut sigaltstack_t>,
) -> Result<(), Errno> {
    let ss_ptr = ss.map_or(core::ptr::null::<sigaltstack_t>() as usize, |ss| {
        ss as *const sigaltstack_t as usize
    });
    let old_ss_ptr = old_ss.map_or(core::ptr::null_mut::<sigaltstack_t>() as usize, |old_ss| {
        old_ss as *mut sigaltstack_t as usize
    });
    syscall2(SYS_SIGALTSTACK, ss_ptr, old_ss_ptr).map(drop)
}

/// Create a file descriptor to accept signals.
pub unsafe fn signalfd4(fd: i32, mask: &sigset_t, flags: i32) -> Result<i32, Errno> {
    let fd = fd as usize;
    let mask_ptr = mask as *const sigset_t as usize;
    let size_mask = core::mem::size_of::<sigset_t>();
    let flags = flags as usize;
    syscall4(SYS_SIGNALFD4, fd, mask_ptr, size_mask, flags).map(|ret| ret as i32)
}

/// Create an endpoint for communication.
///
/// # Examples
///
/// ```
/// let socket_fd = unsafe { nc::socket(nc::AF_INET, nc::SOCK_STREAM, 0) };
/// assert!(socket_fd.is_ok());
/// let socket_fd = socket_fd.unwrap();
/// let ret = unsafe { nc::close(socket_fd) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn socket(domain: i32, sock_type: i32, protocol: i32) -> Result<i32, Errno> {
    let domain = domain as usize;
    let sock_type = sock_type as usize;
    let protocol = protocol as usize;
    syscall3(SYS_SOCKET, domain, sock_type, protocol).map(|ret| ret as i32)
}

/// Create a pair of connected socket.
///
/// # Examples
///
/// ```
/// use std::thread;
///
/// const READ_SIDE: usize = 0;
/// const WRITE_SIDE: usize = 1;
///
/// let mut fds = [-1_i32; 2];
///
/// let ret = unsafe { nc::socketpair(nc::AF_UNIX, nc::SOCK_STREAM, 0, &mut fds) };
/// assert!(ret.is_ok());
/// println!("socket pairs: {}, {}", fds[0], fds[1]);
///
/// // Start worker thread
/// thread::spawn(move || {
///     println!("worker thread started");
///     let msg = "Hello, Rust";
///     println!("[worker] Will send msg: {msg}");
///     let ret = unsafe { nc::write(fds[WRITE_SIDE], msg.as_bytes()) };
///     assert!(ret.is_ok());
///     assert_eq!(ret, Ok(msg.len() as nc::ssize_t));
/// });
///
/// let mut buf = [0_u8; 1024];
/// let ret = unsafe { nc::read(fds[READ_SIDE], &mut buf) };
/// assert!(ret.is_ok());
/// let nread = ret.unwrap();
/// let msg = std::str::from_utf8(&buf[..nread as usize]).unwrap();
/// println!("[main] recv msg: {msg}");
///
/// unsafe {
///     let _ = nc::close(fds[0]);
///     let _ = nc::close(fds[1]);
/// }
/// ```
pub unsafe fn socketpair(
    domain: i32,
    type_: i32,
    protocol: i32,
    pair: &mut [i32; 2],
) -> Result<(), Errno> {
    let domain = domain as usize;
    let type_ = type_ as usize;
    let protocol = protocol as usize;
    let pair_ptr = pair.as_mut_ptr() as usize;
    syscall4(SYS_SOCKETPAIR, domain, type_, protocol, pair_ptr).map(drop)
}

/// Splice data to/from pipe.
///
/// # Examples
///
/// ```
/// let mut fds_left = [0, 0];
/// let ret = unsafe { nc::pipe2(&mut fds_left, 0) };
/// assert!(ret.is_ok());
///
/// let mut fds_right = [0, 0];
/// let ret = unsafe { nc::pipe2(&mut fds_right, 0) };
/// assert!(ret.is_ok());
///
/// let msg = "Hello, Rust";
/// let ret = unsafe { nc::write(fds_left[1], msg.as_bytes()) };
/// assert!(ret.is_ok());
/// let n_write = ret.unwrap() as nc::size_t;
/// assert_eq!(n_write, msg.len());
///
/// let ret = unsafe {
///     nc::splice(
///         fds_left[0],
///         None,
///         fds_right[1],
///         None,
///         n_write,
///         nc::SPLICE_F_MOVE,
///     )
/// };
/// assert!(ret.is_ok());
///
/// let mut buf = [0u8; 64];
/// let buf_len = buf.len();
/// let ret = unsafe { nc::read(fds_right[0], &mut buf) };
/// assert!(ret.is_ok());
/// let n_read = ret.unwrap() as nc::size_t;
/// assert_eq!(n_read, n_write);
/// let read_msg = std::str::from_utf8(&buf[..n_read]);
/// assert!(read_msg.is_ok());
/// assert_eq!(Ok(msg), read_msg);
///
/// unsafe {
///     assert!(nc::close(fds_left[0]).is_ok());
///     assert!(nc::close(fds_left[1]).is_ok());
///     assert!(nc::close(fds_right[0]).is_ok());
///     assert!(nc::close(fds_right[1]).is_ok());
/// }
/// ```
pub unsafe fn splice(
    fd_in: i32,
    off_in: Option<&mut loff_t>,
    fd_out: i32,
    off_out: Option<&mut loff_t>,
    len: size_t,
    flags: u32,
) -> Result<ssize_t, Errno> {
    use core::ptr::null_mut;
    let fd_in = fd_in as usize;
    let off_in_ptr = off_in.map_or(null_mut::<loff_t>() as usize, |off_in| {
        off_in as *mut loff_t as usize
    });
    let fd_out = fd_out as usize;
    let off_out_ptr = off_out.map_or(null_mut::<loff_t>() as usize, |off_out| {
        off_out as *mut loff_t as usize
    });
    let flags = flags as usize;
    syscall6(
        SYS_SPLICE,
        fd_in,
        off_in_ptr,
        fd_out,
        off_out_ptr,
        len,
        flags,
    )
    .map(|ret| ret as ssize_t)
}

/// Get filesystem statistics.
///
/// # Examples
///
/// ```
/// let path = "/usr";
/// let mut statfs = nc::statfs_t::default();
/// let ret = unsafe { nc::statfs(path, &mut statfs) };
/// assert!(ret.is_ok());
/// assert!(statfs.f_bfree > 0);
/// assert!(statfs.f_bavail > 0);
/// ```
pub unsafe fn statfs<P: AsRef<Path>>(filename: P, buf: &mut statfs_t) -> Result<(), Errno> {
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    let buf_ptr = buf as *mut statfs_t as usize;
    syscall2(SYS_STATFS, filename_ptr, buf_ptr).map(drop)
}

pub unsafe fn statmount(
    req: &mnt_id_req_t,
    buf: &mut [statmount_t],
    flags: u32,
) -> Result<(), Errno> {
    let req_ptr = req as *const mnt_id_req_t as usize;
    let buf_ptr = buf.as_mut_ptr() as usize;
    let buf_size = buf.len();
    let flags = flags as usize;
    syscall4(SYS_STATMOUNT, req_ptr, buf_ptr, buf_size, flags).map(drop)
}

/// Get file status about a file (extended).
///
/// # Examples
///
/// ```
/// let path = "/etc/passwd";
/// let mut statx = nc::statx_t::default();
/// let ret = unsafe { nc::statx(nc::AT_FDCWD, path, nc::AT_SYMLINK_NOFOLLOW, nc::STATX_TYPE, &mut statx) };
/// assert!(ret.is_ok());
/// // Check fd is a regular file.
/// assert!(nc::S_ISREG(statx.stx_mode as nc::mode_t));
/// ```
pub unsafe fn statx<P: AsRef<Path>>(
    dirfd: i32,
    filename: P,
    flags: i32,
    mask: u32,
    buf: &mut statx_t,
) -> Result<(), Errno> {
    let dirfd = dirfd as usize;
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    let flags = flags as usize;
    let mask = mask as usize;
    let buf_ptr = buf as *mut statx_t as usize;
    syscall5(SYS_STATX, dirfd, filename_ptr, flags, mask, buf_ptr).map(drop)
}

/// Stop swapping to file/device.
///
/// # Examples
///
/// ```
/// let filename = "/dev/sda-no-exist";
/// let ret = unsafe { nc::swapoff(filename) };
/// assert!(ret.is_err());
/// assert_eq!(ret, Err(nc::EPERM));
/// ```
pub unsafe fn swapoff<P: AsRef<Path>>(filename: P) -> Result<(), Errno> {
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    syscall1(SYS_SWAPOFF, filename_ptr).map(drop)
}

/// Start swapping to file/device.
///
/// # Examples
///
/// ```
/// let filename = "/dev/sda-no-exist";
/// let ret = unsafe { nc::swapon(filename, nc::SWAP_FLAG_PREFER) };
/// assert!(ret.is_err());
/// assert_eq!(ret, Err(nc::EPERM));
/// ```
pub unsafe fn swapon<P: AsRef<Path>>(filename: P, flags: i32) -> Result<(), Errno> {
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    let flags = flags as usize;
    syscall2(SYS_SWAPON, filename_ptr, flags).map(drop)
}

/// Make a new name for a file.
///
/// # Examples
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
    old_name: P,
    new_dirfd: i32,
    new_name: P,
) -> Result<(), Errno> {
    let old_name = CString::new(old_name.as_ref());
    let old_name_ptr = old_name.as_ptr() as usize;
    let new_dirfd = new_dirfd as usize;
    let new_name = CString::new(new_name.as_ref());
    let new_name_ptr = new_name.as_ptr() as usize;
    syscall3(SYS_SYMLINKAT, old_name_ptr, new_dirfd, new_name_ptr).map(drop)
}

/// Commit filesystem caches to disk.
///
/// # Examples
///
/// ```
/// let ret = unsafe { nc::sync() };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn sync() -> Result<(), Errno> {
    syscall0(SYS_SYNC).map(drop)
}

/// Commit filesystem cache related to `fd` to disk.
///
/// # Examples
///
/// ```
/// let path = "/etc/passwd";
/// let ret = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_RDONLY, 0) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
/// let ret = unsafe {nc::syncfs(fd) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn syncfs(fd: i32) -> Result<(), Errno> {
    let fd = fd as usize;
    syscall1(SYS_SYNCFS, fd).map(drop)
}

/// Sync a file segment to disk
///
/// # Examples
///
/// ```
/// let path = "/tmp/nc-sync-file-range";
/// let ret = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_WRONLY | nc::O_CREAT, 0o644) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
///
/// let msg = b"Hello, Rust";
/// let ret = unsafe { nc::write(fd, msg) };
/// assert!(ret.is_ok());
/// let n_write = ret.unwrap();
/// assert_eq!(n_write, msg.len() as nc::ssize_t);
///
/// let ret = unsafe {
///     nc::sync_file_range(
///         fd,
///         0,
///         n_write,
///         nc::SYNC_FILE_RANGE_WAIT_BEFORE
///         | nc::SYNC_FILE_RANGE_WRITE
///         | nc::SYNC_FILE_RANGE_WAIT_AFTER,
///     )
/// };
/// assert!(ret.is_ok());
///
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, path, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn sync_file_range(
    fd: i32,
    offset: off_t,
    nbytes: off_t,
    flags: i32,
) -> Result<(), Errno> {
    let fd = fd as usize;
    let offset = offset as usize;
    let nbytes = nbytes as usize;
    let flags = flags as usize;
    syscall4(SYS_SYNC_FILE_RANGE, fd, offset, nbytes, flags).map(drop)
}

/// Return system information.
///
/// # Examples
///
/// ```
/// let mut info = nc::sysinfo_t::default();
/// let ret = unsafe { nc::sysinfo(&mut info) };
/// assert!(ret.is_ok());
/// assert!(info.uptime > 0);
/// assert!(info.freeram > 0);
/// ```
pub unsafe fn sysinfo(info: &mut sysinfo_t) -> Result<(), Errno> {
    let info_ptr = info as *mut sysinfo_t as usize;
    syscall1(SYS_SYSINFO, info_ptr).map(drop)
}

/// Read and/or clear kernel message ring buffer.
///
/// # Examples
///
/// ```
/// let mut buf = vec![0_u8; 4096];
/// let ret = unsafe { nc::syslog(nc::SYSLOG_ACTION_READ_ALL, &mut buf) };
/// if let Ok(nread) = ret {
///     if let Ok(msg) = std::str::from_utf8(&buf[..nread as usize]) {
///         println!("msg: {msg}");
///     }
/// } else {
///     assert_eq!(ret, Err(nc::EPERM));
/// }
/// ```
pub unsafe fn syslog(action: i32, buf: &mut [u8]) -> Result<ssize_t, Errno> {
    let action = action as usize;
    let buf_ptr = buf.as_mut_ptr() as usize;
    let buf_len = buf.len();
    syscall3(SYS_SYSLOG, action, buf_ptr, buf_len).map(|ret| ret as ssize_t)
}

/// Duplicate pipe content.
///
/// # Examples
///
/// ```
/// let mut fds_left = [0, 0];
/// let ret = unsafe { nc::pipe2(&mut fds_left, 0) };
/// assert!(ret.is_ok());
///
/// let mut fds_right = [0, 0];
/// let ret = unsafe { nc::pipe2(&mut fds_right, 0) };
/// assert!(ret.is_ok());
///
/// let msg = "Hello, Rust";
/// let ret = unsafe { nc::write(fds_left[1], msg.as_bytes()) };
/// assert!(ret.is_ok());
/// let n_write = ret.unwrap() as nc::size_t;
/// assert_eq!(n_write, msg.len());
///
/// let ret = unsafe { nc::tee(fds_left[0], fds_right[1], n_write, nc::SPLICE_F_NONBLOCK) };
/// assert!(ret.is_ok());
///
/// let mut buf = [0u8; 64];
/// let buf_len = buf.len();
/// let ret = unsafe { nc::read(fds_right[0], &mut buf) };
/// assert!(ret.is_ok());
/// let n_read = ret.unwrap() as nc::size_t;
/// assert_eq!(n_read, n_write);
/// let read_msg = std::str::from_utf8(&buf[..n_read]);
/// assert!(read_msg.is_ok());
/// assert_eq!(Ok(msg), read_msg);
///
/// unsafe {
///     assert!(nc::close(fds_left[0]).is_ok());
///     assert!(nc::close(fds_left[1]).is_ok());
///     assert!(nc::close(fds_right[0]).is_ok());
///     assert!(nc::close(fds_right[1]).is_ok());
/// }
/// ```
pub unsafe fn tee(fd_in: i32, fd_out: i32, len: size_t, flags: u32) -> Result<ssize_t, Errno> {
    let fd_in = fd_in as usize;
    let fd_out = fd_out as usize;
    let flags = flags as usize;
    syscall4(SYS_TEE, fd_in, fd_out, len, flags).map(|ret| ret as ssize_t)
}

/// Send a signal to a thread.
///
/// # Examples
///
/// ```
/// let pid = unsafe { nc::fork() };
/// assert!(pid.is_ok());
/// let pid = pid.unwrap();
/// assert!(pid >= 0);
///
/// if pid == 0 {
///     // child process.
///     let mask = nc::sigset_t::default();
///     let ret = unsafe { nc::rt_sigsuspend(&mask) };
///     assert!(ret.is_ok());
/// } else {
///     // parent process.
///     let t = nc::timespec_t {
///         tv_sec: 1,
///         tv_nsec: 0,
///     };
///     let ret = unsafe { nc::nanosleep(&t, None) };
///     assert!(ret.is_ok());
///
///     let ret = unsafe { nc::tgkill(pid, pid, nc::SIGTERM) };
///     assert!(ret.is_ok());
/// }
/// ```
pub unsafe fn tgkill(tgid: i32, tid: i32, sig: i32) -> Result<(), Errno> {
    let tgid = tgid as usize;
    let tid = tid as usize;
    let sig = sig as usize;
    syscall3(SYS_TGKILL, tgid, tid, sig).map(drop)
}

/// Create a timer that notifies via a file descriptor.
///
/// # Examples
///
/// ```
/// let ret = unsafe { nc::timerfd_create(nc::CLOCK_MONOTONIC, nc::TFD_CLOEXEC) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn timerfd_create(clockid: i32, flags: i32) -> Result<i32, Errno> {
    let clockid = clockid as usize;
    let flags = flags as usize;
    syscall2(SYS_TIMERFD_CREATE, clockid, flags).map(|ret| ret as i32)
}

/// Get current timer via a file descriptor.
/// # Examples
///
/// ```
/// let ret = unsafe { nc::timerfd_create(nc::CLOCK_MONOTONIC, nc::TFD_CLOEXEC) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
///
/// let flags = 0;
/// let time = nc::itimerspec_t {
///     it_interval: nc::timespec_t::default(),
///     it_value: nc::timespec_t {
///         tv_sec: 1,
///         tv_nsec: 0,
///     },
/// };
/// let ret = unsafe { nc::timerfd_settime(fd, flags, &time, None) };
/// assert!(ret.is_ok());
///
/// let mut curr_time = nc::itimerspec_t::default();
/// let ret = unsafe { nc::timerfd_gettime(fd, &mut curr_time) };
/// assert!(ret.is_ok());
/// println!("curr time: {curr_time:?}");
///
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn timerfd_gettime(ufd: i32, cur_value: &mut itimerspec_t) -> Result<(), Errno> {
    let ufd = ufd as usize;
    let cur_value_ptr = cur_value as *mut itimerspec_t as usize;
    syscall2(SYS_TIMERFD_GETTIME, ufd, cur_value_ptr).map(drop)
}

/// Set current timer via a file descriptor.
///
/// # Examples
///
/// ```
/// let ret = unsafe { nc::timerfd_create(nc::CLOCK_MONOTONIC, nc::TFD_CLOEXEC) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
///
/// let flags = 0;
/// let time = nc::itimerspec_t {
///     it_interval: nc::timespec_t::default(),
///     it_value: nc::timespec_t {
///         tv_sec: 1,
///         tv_nsec: 0,
///     },
/// };
/// let ret = unsafe { nc::timerfd_settime(fd, flags, &time, None) };
/// assert!(ret.is_ok());
///
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn timerfd_settime(
    ufd: i32,
    flags: i32,
    new_value: &itimerspec_t,
    old_value: Option<&mut itimerspec_t>,
) -> Result<(), Errno> {
    let ufd = ufd as usize;
    let flags = flags as usize;
    let new_value_ptr = new_value as *const itimerspec_t as usize;
    let old_value_ptr = old_value.map_or(
        core::ptr::null_mut::<itimerspec_t>() as usize,
        |old_value| old_value as *mut itimerspec_t as usize,
    );
    syscall4(
        SYS_TIMERFD_SETTIME,
        ufd,
        flags,
        new_value_ptr,
        old_value_ptr,
    )
    .map(drop)
}

/// Create a per-process timer
///
/// # Examples
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
    let event_ptr = event.map_or(core::ptr::null_mut::<sigevent_t>() as usize, |event| {
        event as *mut sigevent_t as usize
    });
    let timer_id_ptr = timer_id as *mut timer_t as usize;
    syscall3(SYS_TIMER_CREATE, clock, event_ptr, timer_id_ptr).map(drop)
}

/// Delete a per-process timer
///
/// # Examples
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
/// # Examples
///
/// ```
/// const TIMER_SIG: i32 = nc::SIGRTMAX;
///
/// fn handle_alarm(signum: i32) {
///     assert_eq!(signum, TIMER_SIG);
/// }
///
/// fn main() {
///     let sa = nc::new_sigaction(handle_alarm);
///     let ret = unsafe { nc::rt_sigaction(TIMER_SIG, Some(&sa), None) };
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
///     let _ret = unsafe { nc::rt_sigsuspend(&mask) };
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

/// Fetch state of per-process timer>
///
/// # Examples
///
/// ```
/// const TIMER_SIG: i32 = nc::SIGRTMAX;
///
/// fn handle_alarm(signum: i32) {
///     assert_eq!(signum, TIMER_SIG);
/// }
///
/// fn main() {
///     let sa = nc::new_sigaction(handle_alarm);
///     let ret = unsafe { nc::rt_sigaction(TIMER_SIG, Some(&sa), None) };
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
///     let _ret = unsafe { nc::rt_sigsuspend(&mask) };
///
///     let ret = unsafe { nc::timer_delete(timer_id) };
///     assert!(ret.is_ok());
/// }
/// ```
pub unsafe fn timer_gettime(timer_id: timer_t, curr: &mut itimerspec_t) -> Result<(), Errno> {
    let timer_id = timer_id as usize;
    let curr_ptr = curr as *mut itimerspec_t as usize;
    syscall2(SYS_TIMER_GETTIME, timer_id, curr_ptr).map(drop)
}

/// Arm/disarm state of per-process timer.
///
/// # Examples
///
/// ```
/// const TIMER_SIG: i32 = nc::SIGRTMAX;
///
/// fn handle_alarm(signum: i32) {
///     assert_eq!(signum, TIMER_SIG);
/// }
///
/// fn main() {
///     let sa = nc::new_sigaction(handle_alarm);
///     let ret = unsafe { nc::rt_sigaction(TIMER_SIG, Some(&sa), None) };
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
///     let _ret = unsafe { nc::rt_sigsuspend(&mask) };
///
///     let ret = unsafe { nc::timer_delete(timer_id) };
///     assert!(ret.is_ok());
/// }
/// ```
pub unsafe fn timer_settime(
    timer_id: timer_t,
    flags: i32,
    new_value: &itimerspec_t,
    old_value: Option<&mut itimerspec_t>,
) -> Result<(), Errno> {
    let timer_id = timer_id as usize;
    let flags = flags as usize;
    let new_value_ptr = new_value as *const itimerspec_t as usize;
    let old_value_ptr = old_value.map_or(
        core::ptr::null_mut::<itimerspec_t>() as usize,
        |old_value| old_value as *mut itimerspec_t as usize,
    );
    syscall4(
        SYS_TIMER_SETTIME,
        timer_id,
        flags,
        new_value_ptr,
        old_value_ptr,
    )
    .map(drop)
}

/// Get process times.
///
/// # Examples
///
/// ```
/// let mut tms = nc::tms_t::default();
/// let ret = unsafe { nc::times(&mut tms) };
/// assert!(ret.is_ok());
/// let clock = ret.unwrap();
/// assert!(clock > 0);
/// ```
pub unsafe fn times(buf: &mut tms_t) -> Result<clock_t, Errno> {
    let buf_ptr = buf as *mut tms_t as usize;
    syscall1(SYS_TIMES, buf_ptr).map(|ret| ret as clock_t)
}

/// Send a signal to a thread (obsolete).
///
/// # Examples
///
/// ```
/// let pid = unsafe { nc::fork() };
/// assert!(pid.is_ok());
/// let pid = pid.unwrap();
/// assert!(pid >= 0);
///
/// if pid == 0 {
///     // child process.
///     let mask = nc::sigset_t::default();
///     let ret = unsafe { nc::rt_sigsuspend(&mask) };
///     assert!(ret.is_ok());
/// } else {
///     // parent process.
///     let t = nc::timespec_t {
///         tv_sec: 1,
///         tv_nsec: 0,
///     };
///     let ret = unsafe { nc::nanosleep(&t, None) };
///     assert!(ret.is_ok());
///
///     let ret = unsafe { nc::tkill(pid, nc::SIGTERM) };
///     assert!(ret.is_ok());
/// }
/// ```
pub unsafe fn tkill(tid: i32, sig: i32) -> Result<(), Errno> {
    let tid = tid as usize;
    let sig = sig as usize;
    syscall2(SYS_TKILL, tid, sig).map(drop)
}

/// Truncate a file to a specified length.
///
/// # Examples
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
/// # Examples
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

/// Unmount filesystem.
///
/// # Examples
///
/// ```
/// let target_dir = "/tmp/nc-umount2";
/// let ret = unsafe { nc::mkdirat(nc::AT_FDCWD, target_dir, 0o755) };
/// assert!(ret.is_ok());
///
/// let src_dir = "/etc";
/// let fs_type = "";
/// let mount_flags = nc::MS_BIND | nc::MS_RDONLY;
/// let data = std::ptr::null_mut();
/// let ret = unsafe { nc::mount(src_dir, target_dir, fs_type, mount_flags, data) };
/// assert!(ret.is_err());
/// assert_eq!(ret, Err(nc::EPERM));
///
/// let flags = nc::UMOUNT_NOFOLLOW;
/// let ret = unsafe { nc::umount2(target_dir, flags) };
/// assert!(ret.is_err());
/// assert_eq!(ret, Err(nc::EPERM));
///
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, target_dir, nc::AT_REMOVEDIR) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn umount2<P: AsRef<Path>>(name: P, flags: u32) -> Result<(), Errno> {
    // NOTE(Shaohua): type of flags is defined as i32 in linux kernel.
    let name = CString::new(name.as_ref());
    let name_ptr = name.as_ptr() as usize;
    let flags = flags as usize;
    syscall2(SYS_UMOUNT2, name_ptr, flags).map(drop)
}

/// Get name and information about current kernel.
///
/// # Examples
///
/// ```
/// let mut buf = nc::utsname_t::default();
/// let ret = unsafe { nc::uname(&mut buf) };
/// assert!(ret.is_ok());
/// assert!(!buf.sysname.is_empty());
/// assert!(!buf.machine.is_empty());
/// ```
pub unsafe fn uname(buf: &mut utsname_t) -> Result<(), Errno> {
    let buf_ptr = buf as *mut utsname_t as usize;
    syscall1(SYS_UNAME, buf_ptr).map(drop)
}

/// Delete a name and possibly the file it refers to.
///
/// # Examples
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

/// Disassociate parts of the process execution context
pub unsafe fn unshare(flags: i32) -> Result<(), Errno> {
    let flags = flags as usize;
    syscall1(SYS_UNSHARE, flags).map(drop)
}

/// Create a file descriptor to handle page faults in user space.
pub unsafe fn userfaultfd(flags: i32) -> Result<i32, Errno> {
    let flags = flags as usize;
    syscall1(SYS_USERFAULTFD, flags).map(|ret| ret as i32)
}

/// Change time timestamps with nanosecond precision.
///
/// # Examples
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

/// Virtually hang up the current terminal.
pub unsafe fn vhangup() -> Result<(), Errno> {
    syscall0(SYS_VHANGUP).map(drop)
}

/// Splice user page into a pipe.
pub unsafe fn vmsplice(fd: i32, iov: &[iovec_t], flags: u32) -> Result<ssize_t, Errno> {
    let fd = fd as usize;
    let iov_ptr = iov.as_ptr() as usize;
    let nr_segs = iov.len();
    let flags = flags as usize;
    syscall4(SYS_VMSPLICE, fd, iov_ptr, nr_segs, flags).map(|ret| ret as ssize_t)
}

/// Wait for process to change state.
///
/// # Examples
///
/// ```
/// let pid = unsafe { nc::fork() };
///
/// match pid {
///     Err(errno) => {
///         eprintln!("fork() error: {}", nc::strerror(errno));
///         unsafe { nc::exit(1) };
///     }
///     Ok(0) => println!("[child] pid is: {}", unsafe { nc::getpid() }),
///     Ok(pid) => {
///         let mut status = 0;
///         let ret = unsafe { nc::wait4(-1, Some(&mut status), 0, None) };
///         assert!(ret.is_ok());
///         println!("status: {}", status);
///         let exited_pid = ret.unwrap();
///         assert_eq!(exited_pid, pid);
///     }
/// }
/// ```
pub unsafe fn wait4(
    pid: pid_t,
    wstatus: Option<&mut i32>,
    options: i32,
    rusage: Option<&mut rusage_t>,
) -> Result<pid_t, Errno> {
    let pid = pid as usize;
    let wstatus_ptr = wstatus.map_or(core::ptr::null_mut::<i32>() as usize, |wstatus| {
        wstatus as *mut i32 as usize
    });
    let options = options as usize;
    let rusage_ptr = rusage.map_or(core::ptr::null_mut::<rusage_t>() as usize, |rusage| {
        rusage as *mut rusage_t as usize
    });
    syscall4(SYS_WAIT4, pid, wstatus_ptr, options, rusage_ptr).map(|ret| ret as pid_t)
}

/// Wait for process to change state.
///
/// # Examples
///
/// ```
/// let pid = unsafe { nc::fork() };
///
/// match pid {
///     Err(errno) => {
///         eprintln!("fork() error: {}", nc::strerror(errno));
///         unsafe { nc::exit(1) };
///     }
///     Ok(0) => println!("[child] pid is: {}", unsafe { nc::getpid() }),
///     Ok(pid) => {
///         let mut info = nc::siginfo_t::default();
///         let options = nc::WEXITED;
///         let ret = unsafe { nc::waitid(nc::P_ALL, -1, &mut info, options, None) };
///         match ret {
///             Err(errno) => eprintln!("waitid() error: {}", nc::strerror(errno)),
///             Ok(()) => {
///                 let exited_pid = unsafe { info.siginfo.sifields.sigchld.pid };
///                 assert_eq!(pid, exited_pid);
///             }
///         }
///     }
/// }
/// ```
pub unsafe fn waitid(
    which: i32,
    pid: pid_t,
    info: &mut siginfo_t,
    options: i32,
    ru: Option<&mut rusage_t>,
) -> Result<(), Errno> {
    let which = which as usize;
    let pid = pid as usize;
    let info_ptr = info as *mut siginfo_t as usize;
    let options = options as usize;
    let ru_ptr = ru.map_or(core::ptr::null_mut::<rusage_t>() as usize, |ru| {
        ru as *mut rusage_t as usize
    });
    syscall5(SYS_WAITID, which, pid, info_ptr, options, ru_ptr).map(drop)
}

/// Write to a file descriptor.
///
/// # Examples
///
/// ```
/// let path = "/tmp/nc-write";
/// let ret = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_CREAT | nc::O_WRONLY, 0o644) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
/// let msg = b"Hello, Rust!";
/// let ret = unsafe { nc::write(fd, msg) };
/// assert!(ret.is_ok());
/// assert_eq!(ret, Ok(msg.len() as nc::ssize_t));
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, path, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn write(fd: i32, buf: &[u8]) -> Result<ssize_t, Errno> {
    let fd = fd as usize;
    let count = buf.len();
    let buf_ptr = buf.as_ptr() as usize;
    syscall3(SYS_WRITE, fd, buf_ptr, count).map(|ret| ret as ssize_t)
}

/// Write to a file descriptor from multiple buffers.
///
/// # Examples
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
/// let ret = unsafe { nc::readv(fd as usize, &mut iov) };
/// assert!(ret.is_ok());
/// assert_eq!(ret, Ok(capacity as nc::ssize_t));
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
///
/// let path_out = "/tmp/nc-writev";
/// let ret = unsafe { nc::openat(nc::AT_FDCWD, path_out, nc::O_WRONLY | nc::O_CREAT, 0o644) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
/// let ret = unsafe { nc::writev(fd as usize, &iov) };
/// assert!(ret.is_ok());
/// assert_eq!(ret, Ok(capacity as nc::ssize_t));
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, path_out, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn writev(fd: usize, iov: &[iovec_t]) -> Result<ssize_t, Errno> {
    let iov_ptr = iov.as_ptr() as usize;
    let len = iov.len();
    syscall3(SYS_WRITEV, fd, iov_ptr, len).map(|ret| ret as ssize_t)
}
