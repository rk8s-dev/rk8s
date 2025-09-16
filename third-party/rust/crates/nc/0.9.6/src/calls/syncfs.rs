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
