/// Sync a file segment with disk.
///
/// # Examples
///
/// ```
/// let path = "/tmp/nc-sync-file-range2";
/// let ret = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_WRONLY | nc::O_CREAT, 0o644) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
///
/// let msg = b"Hello, Rust";
/// let ret = unsafe { nc::write(fd, msg) };
/// assert!(ret.is_ok());
/// let n_write = ret.unwrap();
/// assert_eq!(n_write, msg.len() as nc::ssize_t);
///
/// let flags = 0;
/// let ret = unsafe {
///     nc::sync_file_range2(
///         fd,
///         flags,
///         0,
///         n_write,
///     )
/// };
/// assert!(ret.is_ok());
///
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, path, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn sync_file_range2(
    fd: i32,
    flags: i32,
    offset: loff_t,
    nbytes: loff_t,
) -> Result<(), Errno> {
    let fd = fd as usize;
    let flags = flags as usize;
    let offset = offset as usize;
    let nbytes = nbytes as usize;
    syscall4(SYS_SYNC_FILE_RANGE2, fd, flags, offset, nbytes).map(drop)
}
