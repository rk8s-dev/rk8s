/// List extended attribute names.
///
/// # Examples
///
/// ```
/// let path = "/tmp/nc-flistxattr";
/// let fd = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_WRONLY | nc::O_CREAT, 0o644) };
/// assert!(fd.is_ok());
/// let fd = fd.unwrap();
/// let attr_name = "user.creator";
/// let attr_value = "nc-0.0.1";
/// //let flags = 0;
/// let flags = nc::XATTR_CREATE;
/// let ret = unsafe {
///     nc::setxattr(
///         path,
///         &attr_name,
///         attr_value.as_bytes(),
///         flags,
///     )
/// };
/// assert!(ret.is_ok());
/// let mut buf = [0_u8; 16];
/// let buf_len = buf.len();
/// let ret = unsafe { nc::flistxattr(fd, &mut buf) };
/// let attr_len = ret.unwrap() as usize;
/// assert_eq!(&buf[..attr_len - 1], attr_name.as_bytes());
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, path, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn flistxattr(fd: i32, value: &mut [u8]) -> Result<ssize_t, Errno> {
    let fd = fd as usize;
    let value_ptr = value.as_mut_ptr() as usize;
    let size = value.len();
    syscall3(SYS_FLISTXATTR, fd, value_ptr, size).map(|ret| ret as ssize_t)
}
