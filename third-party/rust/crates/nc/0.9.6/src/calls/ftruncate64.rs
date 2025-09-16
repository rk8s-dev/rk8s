/// Truncate a file to a specific length.
///
/// # Examples
///
/// ```
/// let path = "/tmp/nc-ftruncate64";
/// let ret = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_WRONLY | nc::O_CREAT, 0o644) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
/// let ret = unsafe { nc::ftruncate64(fd, 64 * 1024) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, path, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn ftruncate64(fd: i32, len: loff_t) -> Result<(), Errno> {
    let fd = fd as usize;
    let len = len as usize;
    syscall2(SYS_FTRUNCATE64, fd, len).map(drop)
}
