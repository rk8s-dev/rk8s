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
