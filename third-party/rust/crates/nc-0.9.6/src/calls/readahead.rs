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
