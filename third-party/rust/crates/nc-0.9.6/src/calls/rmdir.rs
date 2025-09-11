/// Delete a directory.
///
/// # Examples
///
/// ```
/// let path = "/tmp/nc-rmdir";
/// let ret = unsafe { nc::mkdirat(nc::AT_FDCWD, path, 0o755) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::rmdir(path) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn rmdir<P: AsRef<Path>>(filename: P) -> Result<(), Errno> {
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    syscall1(SYS_RMDIR, filename_ptr).map(drop)
}
