/// Change timestamp of a file relative to a directory file discriptor.
///
/// # Examples
///
/// ```
/// let path = "/tmp/nc-futimesat";
/// let fd = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_WRONLY | nc::O_CREAT, 0o644) };
/// assert!(fd.is_ok());
/// let fd = fd.unwrap();
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// let times = [
///     nc::timeval_t {
///         tv_sec: 100,
///         tv_usec: 0,
///     },
///     nc::timeval_t {
///         tv_sec: 10,
///         tv_usec: 0,
///     },
/// ];
/// let ret = unsafe { nc::futimesat(nc::AT_FDCWD, path, &times) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, path, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn futimesat<P: AsRef<Path>>(
    dirfd: i32,
    filename: P,
    times: &[timeval_t; 2],
) -> Result<(), Errno> {
    let dirfd = dirfd as usize;
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    let times_ptr = times.as_ptr() as usize;
    syscall3(SYS_FUTIMESAT, dirfd, filename_ptr, times_ptr).map(drop)
}
