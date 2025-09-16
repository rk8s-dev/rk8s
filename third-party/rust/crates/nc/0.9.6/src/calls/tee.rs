/// Duplicate pipe content.
///
/// # Examples
///
/// ```
/// let mut fds_left = [0, 0];
/// let ret = unsafe { nc::pipe2(&mut fds_left, 0) };
/// assert!(ret.is_ok());
///
/// let mut fds_right = [0, 0];
/// let ret = unsafe { nc::pipe2(&mut fds_right, 0) };
/// assert!(ret.is_ok());
///
/// let msg = "Hello, Rust";
/// let ret = unsafe { nc::write(fds_left[1], msg.as_bytes()) };
/// assert!(ret.is_ok());
/// let n_write = ret.unwrap() as nc::size_t;
/// assert_eq!(n_write, msg.len());
///
/// let ret = unsafe { nc::tee(fds_left[0], fds_right[1], n_write, nc::SPLICE_F_NONBLOCK) };
/// assert!(ret.is_ok());
///
/// let mut buf = [0u8; 64];
/// let buf_len = buf.len();
/// let ret = unsafe { nc::read(fds_right[0], &mut buf) };
/// assert!(ret.is_ok());
/// let n_read = ret.unwrap() as nc::size_t;
/// assert_eq!(n_read, n_write);
/// let read_msg = std::str::from_utf8(&buf[..n_read]);
/// assert!(read_msg.is_ok());
/// assert_eq!(Ok(msg), read_msg);
///
/// unsafe {
///     assert!(nc::close(fds_left[0]).is_ok());
///     assert!(nc::close(fds_left[1]).is_ok());
///     assert!(nc::close(fds_right[0]).is_ok());
///     assert!(nc::close(fds_right[1]).is_ok());
/// }
/// ```
pub unsafe fn tee(fd_in: i32, fd_out: i32, len: size_t, flags: u32) -> Result<ssize_t, Errno> {
    let fd_in = fd_in as usize;
    let fd_out = fd_out as usize;
    let flags = flags as usize;
    syscall4(SYS_TEE, fd_in, fd_out, len, flags).map(|ret| ret as ssize_t)
}
