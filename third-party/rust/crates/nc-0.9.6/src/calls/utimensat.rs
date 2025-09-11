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
