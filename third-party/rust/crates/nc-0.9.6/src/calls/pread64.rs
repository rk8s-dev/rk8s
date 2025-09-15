/// Read from a file descriptor without changing file offset.
///
/// # Examples
///
/// ```
/// let path = "/etc/passwd";
/// let ret = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_RDONLY, 0) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
/// let mut buf = [0_u8; 128];
/// let read_count = 64;
/// let ret = unsafe { nc::pread64(fd, &mut buf[..read_count], 0) };
/// assert!(ret.is_ok());
/// assert_eq!(ret, Ok(read_count as nc::ssize_t));
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn pread64(fd: i32, buf: &mut [u8], offset: off_t) -> Result<ssize_t, Errno> {
    let fd = fd as usize;
    let count = buf.len();
    let buf_ptr = buf.as_mut_ptr() as usize;
    let offset = offset as usize;
    syscall4(SYS_PREAD64, fd, buf_ptr, count, offset).map(|ret| ret as ssize_t)
}
