/// Check user's permission for a file.
///
/// It uses the effective user ID and the group access list to authorize the request.
///
/// # Examples
///
/// ```
/// let ret = unsafe { nc::eaccess("/etc/passwd", nc::F_OK) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::eaccess("/etc/passwd", nc::X_OK) };
/// assert!(ret.is_err());
/// ```
pub unsafe fn eaccess<P: AsRef<Path>>(filename: P, mode: i32) -> Result<(), Errno> {
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    let mode = mode as usize;
    syscall2(SYS_EACCESS, filename_ptr, mode).map(drop)
}
