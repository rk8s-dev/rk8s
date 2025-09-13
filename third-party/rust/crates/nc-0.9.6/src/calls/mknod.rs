/// Create a special or ordinary file.
///
/// # Examples
///
/// ```
/// let path = "/tmp/nc-mknod";
/// // Create a named pipe.
/// let ret = unsafe { nc::mknod(path, nc::S_IFIFO | nc::S_IRUSR | nc::S_IWUSR, 0) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, path, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn mknod<P: AsRef<Path>>(filename: P, mode: mode_t, dev: dev_t) -> Result<(), Errno> {
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    let mode = mode as usize;
    let dev = dev as usize;
    syscall3(SYS_MKNOD, filename_ptr, mode, dev).map(drop)
}
