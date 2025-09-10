/// Read from a file descriptor without changing file offset.
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
///         iov_len: item.len(),
///         iov_base: item.as_ptr() as *const c_void,
///     });
/// }
/// let iov_len = iov.len();
/// let ret = unsafe { nc::preadv(fd as usize, &mut iov, 0, iov_len - 1) };
/// assert!(ret.is_ok());
/// assert_eq!(ret, Ok(capacity as nc::ssize_t));
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn preadv(
    fd: usize,
    vec: &mut [iovec_t],
    pos_l: usize,
    pos_h: usize,
) -> Result<ssize_t, Errno> {
    let vec_ptr = vec.as_mut_ptr() as usize;
    let vec_len = vec.len();
    syscall5(SYS_PREADV, fd, vec_ptr, vec_len, pos_l, pos_h).map(|ret| ret as ssize_t)
}
