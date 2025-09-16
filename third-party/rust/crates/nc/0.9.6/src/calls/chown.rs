/// Change ownership of a file.
///
/// # Examples
///
/// ```
/// let filename = "/tmp/nc-chown";
/// let fd = unsafe { nc::openat(nc::AT_FDCWD, filename, nc::O_CREAT | nc::O_WRONLY | nc::O_TRUNC, 0o644) };
/// assert!(fd.is_ok());
/// let fd = fd.unwrap();
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::chown(filename, 0, 0) };
/// assert!(ret.is_err());
/// assert_eq!(ret, Err(nc::EPERM));
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, filename, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn chown<P: AsRef<Path>>(filename: P, user: uid_t, group: gid_t) -> Result<(), Errno> {
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    let user = user as usize;
    let group = group as usize;
    syscall3(SYS_CHOWN, filename_ptr, user, group).map(drop)
}
