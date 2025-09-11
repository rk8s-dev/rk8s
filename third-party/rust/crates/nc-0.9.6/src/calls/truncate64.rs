/// Truncate a file to a specific length.
///
/// # Examples
///
/// ```
/// let path = "/tmp/nc-truncate64";
/// let ret = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_WRONLY | nc::O_CREAT, 0o644) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::truncate64(path, 64 * 1024) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, path, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn truncate64<P: AsRef<Path>>(path: P, len: loff_t) -> Result<(), Errno> {
    let path = CString::new(path.as_ref());
    let path_ptr = path.as_ptr() as usize;
    let len = len as usize;
    syscall2(SYS_TRUNCATE64, path_ptr, len).map(drop)
}
