/// Read from a file descriptor into multiple buffers.
///
/// # Examples
///
/// ```
/// use core::ffi::c_void;
///
/// let path = "/etc/passwd";
/// let ret = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_RDONLY, 0) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
/// let mut buf = [[0_u8; 64]; 4];
/// let capacity = 4 * 64;
/// let mut iov = Vec::with_capacity(buf.len());
/// for ref mut item in (&mut buf).iter() {
///     iov.push(nc::iovec_t {
///         iov_base: item.as_ptr() as *const c_void,
///         iov_len: item.len(),
///     });
/// }
/// let ret = unsafe { nc::readv(fd as usize, &mut iov) };
/// assert!(ret.is_ok());
/// assert_eq!(ret, Ok(capacity as nc::ssize_t));
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn readv(fd: usize, iov: &mut [iovec_t]) -> Result<ssize_t, Errno> {
    let iov_ptr = iov.as_mut_ptr() as usize;
    let len = iov.len();
    syscall3(SYS_READV, fd, iov_ptr, len).map(|ret| ret as ssize_t)
}
