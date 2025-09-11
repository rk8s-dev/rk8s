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
