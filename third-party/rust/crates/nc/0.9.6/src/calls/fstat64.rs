/// Get file status.
///
/// # Examples
///
/// ```
/// let path = "/tmp";
/// // Open folder directly.
/// let fd = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_PATH, 0) };
/// assert!(fd.is_ok());
/// let fd = fd.unwrap();
/// let mut stat = nc::stat64_t::default();
/// let ret = unsafe { nc::fstat64(fd, &mut stat) };
/// assert!(ret.is_ok());
/// // Check fd is a directory.
/// assert!(nc::S_ISREG(stat.st_mode as nc::mode_t));
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn fstat64(fd: i32, statbuf: &mut stat64_t) -> Result<(), Errno> {
    let fd = fd as usize;
    let statbuf_ptr = statbuf as *mut stat64_t as usize;
    syscall2(SYS_FSTAT64, fd, statbuf_ptr).map(drop)
}
