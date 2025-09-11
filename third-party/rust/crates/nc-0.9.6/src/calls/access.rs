/// Check user's permission for a file.
///
/// It uses the real user ID and the group access list to authorize the request.
///
/// # Examples
///
/// ```
/// let ret = unsafe { nc::access("/etc/passwd", nc::F_OK) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::access("/etc/passwd", nc::X_OK) };
/// assert!(ret.is_err());
/// ```
pub unsafe fn access<P: AsRef<Path>>(filename: P, mode: i32) -> Result<(), Errno> {
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    let mode = mode as usize;
    syscall2(SYS_ACCESS, filename_ptr, mode).map(drop)
}
