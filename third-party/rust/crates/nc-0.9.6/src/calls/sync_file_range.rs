/// Sync a file segment to disk
///
/// # Examples
///
/// ```
/// let path = "/tmp/nc-sync-file-range";
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
/// let ret = unsafe {
///     nc::sync_file_range(
///         fd,
///         0,
///         n_write,
///         nc::SYNC_FILE_RANGE_WAIT_BEFORE
///         | nc::SYNC_FILE_RANGE_WRITE
///         | nc::SYNC_FILE_RANGE_WAIT_AFTER,
///     )
/// };
/// assert!(ret.is_ok());
///
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, path, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn sync_file_range(
    fd: i32,
    offset: off_t,
    nbytes: off_t,
    flags: i32,
) -> Result<(), Errno> {
    let fd = fd as usize;
    let offset = offset as usize;
    let nbytes = nbytes as usize;
    let flags = flags as usize;
    syscall4(SYS_SYNC_FILE_RANGE, fd, offset, nbytes, flags).map(drop)
}
