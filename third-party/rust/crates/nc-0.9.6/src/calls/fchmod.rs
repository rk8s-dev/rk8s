/// Change permissions of a file.
///
/// # Examples
///
/// ```
/// let filename = "/tmp/nc-fchmod";
/// let fd = unsafe {
///     nc::openat(
///         nc::AT_FDCWD,
///         filename,
///         nc::O_CREAT | nc::O_WRONLY,
///         0o644
///     )
/// };
/// assert!(fd.is_ok());
/// let fd = fd.unwrap();
/// let ret = unsafe { nc::fchmod(fd, 0o640) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, filename, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn fchmod(fd: i32, mode: mode_t) -> Result<(), Errno> {
    let fd = fd as usize;
    let mode = mode as usize;
    syscall2(SYS_FCHMOD, fd, mode).map(drop)
}
