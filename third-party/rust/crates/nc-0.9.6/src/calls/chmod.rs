/// Change permissions of a file.
///
/// # Examples
///
/// ```
/// let filename = "/tmp/nc-chmod";
/// let fd = unsafe {
///     nc::openat(
///         nc::AT_FDCWD,
///         filename,
///         nc::O_CREAT | nc::O_WRONLY | nc::O_TRUNC,
///         0o644,
///     )
/// };
/// assert!(fd.is_ok());
/// let fd = fd.unwrap();
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::chmod(filename, 0o600) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, filename, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn chmod<P: AsRef<Path>>(filename: P, mode: mode_t) -> Result<(), Errno> {
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    let mode = mode as usize;
    syscall2(SYS_CHMOD, filename_ptr, mode).map(drop)
}
