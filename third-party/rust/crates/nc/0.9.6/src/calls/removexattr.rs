/// Remove an extended attribute.
///
/// # Examples
///
/// ```
/// let path = "/tmp/nc-removexattr";
/// let ret = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_WRONLY | nc::O_CREAT, 0o644) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// let attr_name = "user.creator";
/// let attr_value = "nc-0.0.1";
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
/// let ret = unsafe { nc::removexattr(path, attr_name) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, path, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn removexattr<P: AsRef<Path>>(filename: P, name: P) -> Result<(), Errno> {
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    let name = CString::new(name.as_ref());
    let name_ptr = name.as_ptr() as usize;
    syscall2(SYS_REMOVEXATTR, filename_ptr, name_ptr).map(drop)
}
