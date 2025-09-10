/// Switch process accounting.
///
/// # Examples
///
/// ```
/// let path = "/tmp/nc-acct";
/// let fd = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_WRONLY | nc::O_CREAT, 0o644) };
/// assert!(fd.is_ok());
/// let fd = fd.unwrap();
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::acct(Some(path)) };
/// assert_eq!(ret, Err(nc::EPERM));
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, path, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn acct<P: AsRef<Path>>(filename: Option<P>) -> Result<(), Errno> {
    let filename = filename.map(|filename| CString::new(filename.as_ref()));
    let filename_ptr = filename.map_or(core::ptr::null::<u8>() as usize, |filename| {
        filename.as_ptr() as usize
    });
    syscall1(SYS_ACCT, filename_ptr).map(drop)
}
