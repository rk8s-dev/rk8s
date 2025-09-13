/// Change file last access and modification time.
///
/// # Examples
///
/// ```
/// let path = "/tmp/nc-utime";
/// let ret = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_WRONLY | nc::O_CREAT, 0o644) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// let time = nc::utimbuf_t {
///     actime: 100,
///     modtime: 10,
/// };
/// let ret = unsafe { nc::utime(path, &time) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, path, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn utime<P: AsRef<Path>>(filename: P, times: &utimbuf_t) -> Result<(), Errno> {
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    let times_ptr = times as *const utimbuf_t as usize;
    syscall2(SYS_UTIME, filename_ptr, times_ptr).map(drop)
}
