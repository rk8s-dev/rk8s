/// Open and possibly create a file within a directory.
///
/// # Examples
///
/// ```
/// let path = "/etc/passwd";
/// let ret = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_RDONLY, 0) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn openat<P: AsRef<Path>>(
    dirfd: i32,
    filename: P,
    flags: i32,
    mode: mode_t,
) -> Result<i32, Errno> {
    let dirfd = dirfd as usize;
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    let flags = flags as usize;
    let mode = mode as usize;
    syscall4(SYS_OPENAT, dirfd, filename_ptr, flags, mode).map(|ret| ret as i32)
}
