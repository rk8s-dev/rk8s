/// List extended attribute names.
///
/// # Examples
///
/// ```
/// let path = "/tmp/nc-listxattr";
/// let ret = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_WRONLY | nc::O_CREAT, 0o644) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
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
/// let ret = unsafe { nc::listxattr(path, &mut buf) };
/// let attr_len = ret.unwrap() as usize;
/// assert_eq!(&buf[..attr_len - 1], attr_name.as_bytes());
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, path, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn listxattr<P: AsRef<Path>>(filename: P, value: &mut [u8]) -> Result<ssize_t, Errno> {
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    let value_ptr = value.as_mut_ptr() as usize;
    let size = value.len();
    syscall3(SYS_LISTXATTR, filename_ptr, value_ptr, size).map(|ret| ret as ssize_t)
}
