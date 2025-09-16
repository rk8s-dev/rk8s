/// Set extended attribute value.
///
/// # Examples
///
/// ```
/// let path = "/tmp/nc-lsetxattr";
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
///     nc::lsetxattr(
///         path,
///         &attr_name,
///         attr_value.as_bytes(),
///         flags,
///     )
/// };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, path, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn lsetxattr<P: AsRef<Path>>(
    filename: P,
    name: P,
    value: &[u8],
    flags: i32,
) -> Result<(), Errno> {
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    let name = CString::new(name.as_ref());
    let name_ptr = name.as_ptr() as usize;
    let value_ptr = value.as_ptr() as usize;
    let size = value.len();
    let flags = flags as usize;
    syscall5(
        SYS_LSETXATTR,
        filename_ptr,
        name_ptr,
        value_ptr,
        size,
        flags,
    )
    .map(drop)
}
