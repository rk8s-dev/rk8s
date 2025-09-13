/// Change permissions of a file.
///
/// Like `fchmodat()`, with specifying extra `flags`.
///
/// # Examples
///
/// ```
/// let filename = "/tmp/nc-fchmodat2";
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
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::fchmodat2(nc::AT_FDCWD, filename, 0o600, nc::AT_SYMLINK_NOFOLLOW as u32) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, filename, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn fchmodat2<P: AsRef<Path>>(
    dirfd: i32,
    filename: P,
    mode: mode_t,
    flags: u32,
) -> Result<(), Errno> {
    let dirfd = dirfd as usize;
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    let mode = mode as usize;
    let flags = flags as usize;
    syscall4(SYS_FCHMODAT, dirfd, filename_ptr, mode, flags).map(drop)
}
