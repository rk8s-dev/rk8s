/// Predeclare an access pattern for file data.
///
/// # Examples
///
/// ```
/// let path = "/etc/passwd";
/// let ret = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_RDONLY, 0) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
/// let offset = 0;
/// let len = 4 * 1024;
/// let ret = unsafe { nc::fadvise64_64(fd, offset, len, nc::POSIX_FADV_NORMAL) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn fadvise64_64(fd: i32, offset: loff_t, len: loff_t, advice: i32) -> Result<(), Errno> {
    let fd = fd as usize;
    let offset = offset as usize;
    let len = len as usize;
    let advice = advice as usize;
    syscall4(SYS_FADVISE64_64, fd, offset, len, advice).map(drop)
}
