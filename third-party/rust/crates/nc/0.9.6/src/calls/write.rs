/// Write to a file descriptor.
///
/// # Examples
///
/// ```
/// let path = "/tmp/nc-write";
/// let ret = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_CREAT | nc::O_WRONLY, 0o644) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
/// let msg = b"Hello, Rust!";
/// let ret = unsafe { nc::write(fd, msg) };
/// assert!(ret.is_ok());
/// assert_eq!(ret, Ok(msg.len() as nc::ssize_t));
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, path, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn write(fd: i32, buf: &[u8]) -> Result<ssize_t, Errno> {
    let fd = fd as usize;
    let count = buf.len();
    let buf_ptr = buf.as_ptr() as usize;
    syscall3(SYS_WRITE, fd, buf_ptr, count).map(|ret| ret as ssize_t)
}
