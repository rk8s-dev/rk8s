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
