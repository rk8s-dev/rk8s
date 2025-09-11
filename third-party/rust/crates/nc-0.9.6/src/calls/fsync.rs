/// Flush all modified in-core data refered by `fd` to disk.
///
/// # Examples
///
/// ```
/// let path = "/tmp/nc-fsync";
/// let fd = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_CREAT | nc::O_WRONLY, 0o644) };
/// assert!(fd.is_ok());
/// let fd = fd.unwrap();
/// let buf = b"Hello, Rust";
/// let n_write = unsafe { nc::write(fd, buf) };
/// assert_eq!(n_write, Ok(buf.len() as isize));
/// let ret = unsafe { nc::fsync(fd) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, path, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn fsync(fd: i32) -> Result<(), Errno> {
    let fd = fd as usize;
    syscall1(SYS_FSYNC, fd).map(drop)
}
