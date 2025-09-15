/// Write to a file descriptor without changing file offset.
///
/// # Examples
///
/// ```
/// let path = "/tmp/nc-pwrite64";
/// let ret = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_WRONLY | nc::O_CREAT, 0o644) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
/// let msg = "Hello, Rust";
/// let ret = unsafe { nc::pwrite64(fd, msg.as_bytes(), 0) };
/// assert!(ret.is_ok());
/// assert_eq!(ret, Ok(msg.len() as nc::ssize_t));
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, path, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn pwrite64(fd: i32, buf: &[u8], offset: loff_t) -> Result<ssize_t, Errno> {
    let fd = fd as usize;
    let count = buf.len();
    let buf_ptr = buf.as_ptr() as usize;
    let offset = offset as usize;
    syscall4(SYS_PWRITE64, fd, buf_ptr, count, offset).map(|ret| ret as ssize_t)
}
