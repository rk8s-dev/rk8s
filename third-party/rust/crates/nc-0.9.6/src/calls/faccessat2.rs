/// Check user's permission for a file.
///
/// # Examples
///
/// ```
/// let ret = unsafe { nc::faccessat2(nc::AT_FDCWD, "/etc/passwd", nc::F_OK, nc::AT_SYMLINK_NOFOLLOW) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn faccessat2<P: AsRef<Path>>(
    dfd: i32,
    filename: P,
    mode: i32,
    flags: i32,
) -> Result<(), Errno> {
    let dfd = dfd as usize;
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    let mode = mode as usize;
    let flags = flags as usize;
    syscall4(SYS_FACCESSAT2, dfd, filename_ptr, mode, flags).map(drop)
}
